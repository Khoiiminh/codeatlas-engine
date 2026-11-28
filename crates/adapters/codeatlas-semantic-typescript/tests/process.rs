//! ADR-0015: how the adapter handles the analyzer process, with small fake analyzers written in
//! JavaScript: version checks, crashes, invalid output, a stuck analyzer, cancellation and
//! progress. The real analyzer is tested in `analyzer.rs`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use codeatlas_application::{
    CancelFlag, NeverCancelled, NoProgress, Phase, Progress, ProgressReporter, RunContext,
    SemanticError, SemanticFailureReason, SemanticOutput, SemanticResolver,
};
use codeatlas_semantic_typescript::{AnalyzerCommand, AnalyzerSettings, TypeScriptResolver};
use support::{node, sources};
use tempfile::TempDir;

const HASH: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

/// A fake analyzer: `body` runs after it has read the request into `request`.
fn fake(dir: &TempDir, body: &str) -> PathBuf {
    let script = dir.path().join("fake.mjs");
    let text = format!(
        "const send = (m) => process.stdout.write(JSON.stringify(m) + \"\\n\");\n\
         const file = (path) => send({{ type: \"file\", path, sha256: \"{HASH}\", modules: [], references: [] }});\n\
         const chunks = [];\n\
         for await (const c of process.stdin) chunks.push(c);\n\
         const request = JSON.parse(Buffer.concat(chunks).toString());\n\
         {body}\n"
    );
    fs::write(&script, text).unwrap();
    script
}

fn resolver(node: &Path, script: &Path) -> TypeScriptResolver {
    TypeScriptResolver::new(AnalyzerCommand::node(node, script))
}

fn resolve(resolver: &TypeScriptResolver) -> Result<SemanticOutput, SemanticError> {
    resolver.resolve(
        Path::new("."),
        &sources(&["a.ts", "b.ts", "c.ts"]),
        &RunContext::detached(),
    )
}

const HELLO: &str = "send({ type: \"hello\", protocol: 1 });";

fn failed_with(output: &SemanticOutput, needle: &str) -> Vec<String> {
    output
        .failed
        .iter()
        .map(|f| {
            let SemanticFailureReason::AnalyzerFailed(message) = &f.reason else {
                panic!("{f:?}");
            };
            assert!(message.contains(needle), "{message:?} lacks {needle:?}");
            f.path.to_string()
        })
        .collect()
}

#[test]
fn a_missing_runtime_is_unavailable() {
    let resolver = TypeScriptResolver::new(AnalyzerCommand::node(
        "codeatlas-no-such-runtime",
        "main.ts",
    ));
    let Err(SemanticError::Unavailable(message)) = resolve(&resolver) else {
        panic!("expected Unavailable");
    };
    assert!(message.contains("could not start"), "{message}");
}

#[test]
fn another_protocol_version_is_unavailable() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    let script = fake(&dir, "send({ type: \"hello\", protocol: 99 });");
    let Err(SemanticError::Unavailable(message)) = resolve(&resolver(&node, &script)) else {
        panic!("expected Unavailable");
    };
    assert!(message.contains("protocol 99"), "{message}");
}

#[test]
fn an_analyzer_that_fails_to_start_is_unavailable_with_its_stderr() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("broken.mjs");
    fs::write(
        &script,
        "console.error('Cannot find package typescript'); process.exit(3);",
    )
    .unwrap();
    let Err(SemanticError::Unavailable(message)) = resolve(&resolver(&node, &script)) else {
        panic!("expected Unavailable");
    };
    assert!(
        message.contains("exited before it started") && message.contains("typescript"),
        "{message}"
    );
}

#[test]
fn a_crash_keeps_what_was_reported_and_fails_the_rest() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    let script = fake(
        &dir,
        &format!(
            "{HELLO} file(\"a.ts\"); console.error(\"heap out of memory\"); process.exit(134);"
        ),
    );
    let output = resolve(&resolver(&node, &script)).unwrap();
    assert_eq!(output.files.len(), 1);
    assert_eq!(output.files[0].path.as_str(), "a.ts");
    assert_eq!(failed_with(&output, "heap out of memory"), ["b.ts", "c.ts"]);
    assert!(output.failed.iter().all(|f| matches!(
        &f.reason,
        SemanticFailureReason::AnalyzerFailed(m) if m.contains("exited before it finished")
    )));
}

#[test]
fn a_fatal_message_fails_the_rest() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    let script = fake(
        &dir,
        &format!(
            "{HELLO} file(\"b.ts\"); send({{ type: \"failed\", path: \"a.ts\", reason: \"unreadable\", message: \"EACCES\" }}); send({{ type: \"fatal\", message: \"worker ran out of memory\" }});"
        ),
    );
    let output = resolve(&resolver(&node, &script)).unwrap();
    assert_eq!(output.files[0].path.as_str(), "b.ts");
    assert_eq!(
        output.failed[0].reason,
        SemanticFailureReason::Unreadable("EACCES".into())
    );
    let rest = SemanticOutput {
        failed: output.failed[1..].to_vec(),
        ..SemanticOutput::default()
    };
    assert_eq!(
        failed_with(&rest, "the analyzer stopped: worker ran out of memory"),
        ["c.ts"]
    );
}

#[test]
fn an_invalid_message_stops_the_analyzer() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    let script = fake(
        &dir,
        &format!("{HELLO} process.stdout.write(\"not json\\n\"); setInterval(() => {{}}, 1000);"),
    );
    let started = Instant::now();
    let output = resolve(&resolver(&node, &script)).unwrap();
    assert_eq!(
        failed_with(&output, "invalid message"),
        ["a.ts", "b.ts", "c.ts"]
    );
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn a_stuck_analyzer_is_stopped_after_the_stall_timeout() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    let script = fake(
        &dir,
        &format!("{HELLO} file(\"a.ts\"); setInterval(() => {{}}, 1000);"),
    );
    let resolver = resolver(&node, &script).with_settings(AnalyzerSettings {
        stall_timeout: Duration::from_millis(500),
        ..AnalyzerSettings::default()
    });
    let started = Instant::now();
    let output = resolve(&resolver).unwrap();
    assert_eq!(
        failed_with(&output, "made no progress for 0.5 s"),
        ["b.ts", "c.ts"]
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(500) && elapsed < Duration::from_secs(20),
        "{elapsed:?}"
    );
}

#[test]
fn cancellation_kills_the_analyzer_within_the_budget() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    // The fake appends to a file every 20 ms for as long as it lives.
    let beat = dir.path().join("beat.txt");
    let script = fake(
        &dir,
        &format!(
            "{HELLO} const fs = await import(\"node:fs\"); setInterval(() => fs.appendFileSync({:?}, \"x\"), 20);",
            beat.to_str().unwrap()
        ),
    );
    let resolver = resolver(&node, &script);
    let flag = CancelFlag::new();
    let run = RunContext::new(&flag, &NoProgress);
    let (result, latency) = thread::scope(|s| {
        let worker = s.spawn(|| {
            let result = resolver.resolve(Path::new("."), &sources(&["a.ts"]), &run);
            (result, Instant::now())
        });
        // Wait until the fake is running, then cancel.
        let deadline = Instant::now() + Duration::from_secs(20);
        while fs::metadata(&beat).map_or(0, |m| m.len()) < 3 {
            assert!(Instant::now() < deadline, "the fake never started");
            thread::sleep(Duration::from_millis(10));
        }
        let cancelled_at = Instant::now();
        flag.cancel();
        let (result, returned_at) = worker.join().unwrap();
        (result, returned_at - cancelled_at)
    });
    assert_eq!(result, Err(SemanticError::Cancelled));
    // ADR-0014: cancellation takes effect within 1 s.
    assert!(latency < Duration::from_secs(1), "{latency:?}");
    // The process is gone: the heartbeat stops.
    thread::sleep(Duration::from_millis(200));
    let before = fs::metadata(&beat).unwrap().len();
    thread::sleep(Duration::from_millis(300));
    assert_eq!(fs::metadata(&beat).unwrap().len(), before);
}

#[test]
fn progress_counts_each_reported_file_and_extra_output_is_ignored() {
    let Some(node) = node() else { return };
    let dir = tempfile::tempdir().unwrap();
    let request_copy = dir.path().join("request.json");
    let script = fake(
        &dir,
        &format!(
            "(await import(\"node:fs\")).writeFileSync({:?}, JSON.stringify(request));\n\
             console.error(\"some warning for people\");\n\
             send({{ type: \"hello\", protocol: 1, analyzer: \"9.9.9\", typescript: \"9.9\", extra: true }});\n\
             send({{ type: \"project\", config: \"tsconfig.json\", files: 3 }});\n\
             process.stdout.write(\"\\n\");\n\
             file(\"a.ts\"); file(\"b.ts\");\n\
             send({{ type: \"failed\", path: \"c.ts\", reason: \"analyzer_failed\", message: \"x\" }});\n\
             send({{ type: \"done\", peakRssBytes: 1234 }});",
            request_copy.to_str().unwrap()
        ),
    );
    struct Events(Mutex<Vec<Progress>>);
    impl ProgressReporter for Events {
        fn report(&self, progress: Progress) {
            self.0.lock().unwrap().push(progress);
        }
    }
    let events = Events(Mutex::new(Vec::new()));
    let resolver = resolver(&node, &script).with_settings(AnalyzerSettings {
        stack_size_mb: Some(64),
        ..AnalyzerSettings::default()
    });
    let output = resolver
        .resolve(
            dir.path(),
            &sources(&["a.ts", "b.ts", "c.ts"]),
            &RunContext::new(&NeverCancelled, &events),
        )
        .unwrap();
    assert_eq!(output.files.len(), 2);
    assert_eq!(output.failed.len(), 1);
    assert_eq!(output.peak_memory_bytes, Some(1234));
    let seen: Vec<(Phase, usize, Option<usize>)> = events
        .0
        .into_inner()
        .unwrap()
        .iter()
        .map(|p| (p.phase, p.done, p.total))
        .collect();
    assert_eq!(
        seen,
        (1..=3)
            .map(|n| (Phase::Resolve, n, Some(3)))
            .collect::<Vec<_>>()
    );
    // The request names an absolute root, the files and the settings.
    let request: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&request_copy).unwrap()).unwrap();
    assert_eq!(request["protocol"], 1);
    assert_eq!(
        request["files"],
        serde_json::json!(["a.ts", "b.ts", "c.ts"])
    );
    assert_eq!(request["stackSizeMb"], 64);
    assert!(Path::new(request["root"].as_str().unwrap()).is_absolute());
}
