//! codeatlas-semantic-typescript: Adapter (driven): SemanticResolver backed by packages/typescript-analyzer.
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).
//!
//! [`TypeScriptResolver`] runs the analyzer once per call (ADR-0015): it starts the Node
//! program, writes the request to its stdin, and reads one JSON message per line from its
//! stdout (protocol in [`wire`]). The analyzer's stderr is kept for error messages only.
//!
//! - **Cancellation** (ADR-0014): the adapter polls the run's token while it waits for output
//!   and kills the process when the run is cancelled, so cancellation takes effect within a
//!   tenth of a second, even in the middle of a long compiler step.
//! - **Failures never stop a run.** An analyzer that cannot start, or speaks another protocol
//!   version, is `SemanticError::Unavailable`. One that crashes, reports a fatal error, sends
//!   an invalid message or makes no progress for [`AnalyzerSettings::stall_timeout`] is killed;
//!   the files it already reported keep their results and every other file is failed with
//!   the reason.

mod wire;

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use codeatlas_application::{
    Phase, RunContext, SemanticError, SemanticFailure, SemanticFailureReason, SemanticOutput,
    SemanticResolver,
};
use codeatlas_core::{RepoPath, SourceFile};

pub use wire::PROTOCOL_VERSION;

/// How long the analyzer may go without a message before it is considered stuck. Building a
/// program for a very large project is one long step without output, so this is generous.
pub const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(300);

/// How often the adapter checks for cancellation while it waits for the analyzer.
const POLL: Duration = Duration::from_millis(50);

/// The last part of the analyzer's stderr that error messages quote.
const STDERR_TAIL_BYTES: usize = 4096;

/// The program that runs the analyzer, for example `node <engine>/packages/typescript-analyzer/src/main.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

impl AnalyzerCommand {
    /// `node <script>`. `node` may be a bare name, looked up on `PATH`.
    pub fn node(node: impl Into<PathBuf>, script: impl Into<PathBuf>) -> Self {
        AnalyzerCommand {
            program: node.into(),
            args: vec![script.into().into_os_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerSettings {
    pub stall_timeout: Duration,
    /// Stack of the analysis thread, in MiB; `None` keeps the analyzer's default (256 MiB).
    pub stack_size_mb: Option<u32>,
    /// Heap limit of the analysis thread, in MiB; `None` keeps Node's default.
    pub max_memory_mb: Option<u32>,
}

impl Default for AnalyzerSettings {
    fn default() -> Self {
        AnalyzerSettings {
            stall_timeout: DEFAULT_STALL_TIMEOUT,
            stack_size_mb: None,
            max_memory_mb: None,
        }
    }
}

/// [`SemanticResolver`] backed by the TypeScript compiler, out of process (ADR-0010 rule 6).
#[derive(Debug, Clone)]
pub struct TypeScriptResolver {
    command: AnalyzerCommand,
    settings: AnalyzerSettings,
}

impl TypeScriptResolver {
    pub fn new(command: AnalyzerCommand) -> Self {
        TypeScriptResolver {
            command,
            settings: AnalyzerSettings::default(),
        }
    }

    pub fn with_settings(self, settings: AnalyzerSettings) -> Self {
        TypeScriptResolver { settings, ..self }
    }
}

impl SemanticResolver for TypeScriptResolver {
    fn resolve(
        &self,
        root: &Path,
        files: &[SourceFile],
        run: &RunContext<'_>,
    ) -> Result<SemanticOutput, SemanticError> {
        if run.is_cancelled() {
            return Err(SemanticError::Cancelled);
        }
        let root = std::path::absolute(root).map_err(|e| {
            SemanticError::Unavailable(format!("the root has no absolute path: {e}"))
        })?;
        let root_text = root.to_str().ok_or_else(|| {
            SemanticError::Unavailable("the root path is not valid Unicode".into())
        })?;
        let request = serde_json::to_vec(&wire::Request {
            protocol: PROTOCOL_VERSION,
            root: root_text,
            files: files.iter().map(|f| f.path.as_str()).collect(),
            stack_size_mb: self.settings.stack_size_mb,
            max_memory_mb: self.settings.max_memory_mb,
        })
        .map_err(|e| {
            SemanticError::Unavailable(format!("the request could not be written: {e}"))
        })?;

        let mut child = self.spawn()?;
        let session = Session::start(&mut child, request);
        let outcome = self.read(&session, files, run);
        // Whatever happened, the process must not outlive the call.
        let _ = child.kill();
        let status = child.wait().ok();
        let stderr = session.stderr();
        match outcome {
            Outcome::Cancelled => Err(SemanticError::Cancelled),
            Outcome::NoHello(why) => Err(SemanticError::Unavailable(match why {
                Some(why) => why,
                None => with_stderr(
                    format!(
                        "the analyzer exited before it started ({})",
                        status.map_or_else(|| "no exit status".into(), |s| s.to_string())
                    ),
                    &stderr,
                ),
            })),
            Outcome::Finished(output) => Ok(output),
            Outcome::Stopped(mut output, why) => {
                let reason = match why {
                    Stop::Fatal(message) => format!("the analyzer stopped: {message}"),
                    Stop::Invalid(message) => {
                        format!("the analyzer sent an invalid message ({message})")
                    }
                    Stop::Stalled => format!(
                        "the analyzer made no progress for {} and was stopped",
                        seconds(self.settings.stall_timeout)
                    ),
                    Stop::Exited => with_stderr(
                        format!(
                            "the analyzer exited before it finished ({})",
                            status.map_or_else(|| "no exit status".into(), |s| s.to_string())
                        ),
                        &stderr,
                    ),
                };
                fail_unreported(&mut output, files, &reason);
                Ok(output)
            }
        }
    }
}

enum Stop {
    Fatal(String),
    Invalid(String),
    Stalled,
    Exited,
}

enum Outcome {
    Finished(SemanticOutput),
    Stopped(SemanticOutput, Stop),
    /// The analyzer never said hello; `Some` when it said something else instead.
    NoHello(Option<String>),
    Cancelled,
}

enum Event {
    Line(String),
    End,
}

/// The pipes of one analyzer process.
struct Session {
    lines: mpsc::Receiver<Event>,
    stderr: thread::JoinHandle<Vec<u8>>,
}

impl Session {
    fn start(child: &mut Child, request: Vec<u8>) -> Session {
        // Write the request on its own thread: the analyzer writes to stdout before it reads
        // stdin, and a large request must not wait for us to read.
        if let Some(mut stdin) = child.stdin.take() {
            thread::spawn(move || {
                let _ = stdin.write_all(&request);
            });
        }
        let (sender, lines) = mpsc::channel();
        if let Some(stdout) = child.stdout.take() {
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if sender.send(Event::Line(line)).is_err() {
                        return;
                    }
                }
                let _ = sender.send(Event::End);
            });
        }
        let stderr = child.stderr.take();
        let stderr = thread::spawn(move || {
            let mut tail = Vec::new();
            if let Some(mut stderr) = stderr {
                let mut chunk = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    tail.extend_from_slice(&chunk[..n]);
                    if tail.len() > 2 * STDERR_TAIL_BYTES {
                        tail.drain(..tail.len() - STDERR_TAIL_BYTES);
                    }
                }
            }
            tail
        });
        Session { lines, stderr }
    }

    fn stderr(self) -> String {
        let bytes = self.stderr.join().unwrap_or_default();
        let start = bytes.len().saturating_sub(STDERR_TAIL_BYTES);
        String::from_utf8_lossy(&bytes[start..]).trim().to_string()
    }
}

impl TypeScriptResolver {
    fn spawn(&self) -> Result<Child, SemanticError> {
        let mut command = Command::new(&self.command.program);
        command
            .args(&self.command.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            // No console window when a windowless host (the VS Code extension) runs the engine.
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command.spawn().map_err(|e| {
            SemanticError::Unavailable(format!(
                "could not start {}: {e}",
                self.command.program.display()
            ))
        })
    }

    fn read(&self, session: &Session, files: &[SourceFile], run: &RunContext<'_>) -> Outcome {
        let total = files.len();
        let mut output = SemanticOutput::default();
        let mut hello = false;
        let mut done = 0;
        let mut last_message = Instant::now();
        loop {
            let event = match session.lines.recv_timeout(POLL) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => {
                    if run.is_cancelled() {
                        return Outcome::Cancelled;
                    }
                    if last_message.elapsed() >= self.settings.stall_timeout {
                        return if hello {
                            Outcome::Stopped(output, Stop::Stalled)
                        } else {
                            Outcome::NoHello(Some(format!(
                                "the analyzer did not start within {}",
                                seconds(self.settings.stall_timeout)
                            )))
                        };
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => Event::End,
            };
            if run.is_cancelled() {
                return Outcome::Cancelled;
            }
            let line = match event {
                Event::Line(line) => line,
                Event::End if hello => return Outcome::Stopped(output, Stop::Exited),
                Event::End => return Outcome::NoHello(None),
            };
            last_message = Instant::now();
            if line.trim().is_empty() {
                continue;
            }
            let message: wire::Message = match serde_json::from_str(&line) {
                Ok(message) => message,
                Err(e) if hello => return Outcome::Stopped(output, Stop::Invalid(e.to_string())),
                Err(e) => {
                    return Outcome::NoHello(Some(format!(
                        "the analyzer did not answer in protocol {PROTOCOL_VERSION} ({e})"
                    )));
                }
            };
            if !hello {
                match message {
                    wire::Message::Hello { protocol } if protocol == PROTOCOL_VERSION => {
                        hello = true;
                        continue;
                    }
                    wire::Message::Hello { protocol } => {
                        return Outcome::NoHello(Some(format!(
                            "the analyzer speaks protocol {protocol}; this engine needs {PROTOCOL_VERSION}"
                        )));
                    }
                    _ => {
                        return Outcome::NoHello(Some(
                            "the analyzer did not say hello first".into(),
                        ));
                    }
                }
            }
            let converted = match message {
                wire::Message::Hello { .. } => Err("a second hello".to_string()),
                wire::Message::Project {} => Ok(false),
                wire::Message::Problem {
                    path,
                    range,
                    message,
                } => wire::problem(path, range, message).map(|p| {
                    output.problems.push(p);
                    false
                }),
                wire::Message::File {
                    path,
                    sha256,
                    modules,
                    references,
                } => wire::resolved_file(path, &sha256, modules, references).map(|f| {
                    output.files.push(f);
                    true
                }),
                wire::Message::Failed {
                    path,
                    reason,
                    message,
                } => wire::failure(path, reason, message).map(|f| {
                    output.failed.push(f);
                    true
                }),
                wire::Message::Done { peak_rss_bytes } => {
                    output.peak_memory_bytes = Some(peak_rss_bytes);
                    return Outcome::Finished(output);
                }
                wire::Message::Fatal { message } => {
                    return Outcome::Stopped(output, Stop::Fatal(message));
                }
            };
            match converted {
                Ok(true) => {
                    done += 1;
                    run.report(Phase::Resolve, done.min(total), Some(total));
                }
                Ok(false) => {}
                Err(e) => return Outcome::Stopped(output, Stop::Invalid(e)),
            }
        }
    }
}

/// Every file the analyzer did not report on fails with `reason`.
fn fail_unreported(output: &mut SemanticOutput, files: &[SourceFile], reason: &str) {
    let reported: std::collections::BTreeSet<&RepoPath> = output
        .files
        .iter()
        .map(|f| &f.path)
        .chain(output.failed.iter().map(|f| &f.path))
        .collect();
    let missing: Vec<SemanticFailure> = files
        .iter()
        .filter(|f| !reported.contains(&f.path))
        .map(|f| SemanticFailure {
            path: f.path.clone(),
            reason: SemanticFailureReason::AnalyzerFailed(reason.to_string()),
        })
        .collect();
    output.failed.extend(missing);
}

/// `300 s`, or `0.5 s` for a duration that is not a whole number of seconds.
fn seconds(d: Duration) -> String {
    if d.subsec_millis() == 0 {
        format!("{} s", d.as_secs())
    } else {
        format!("{:.1} s", d.as_secs_f64())
    }
}

fn with_stderr(message: String, stderr: &str) -> String {
    if stderr.is_empty() {
        message
    } else {
        format!("{message}: {stderr}")
    }
}
