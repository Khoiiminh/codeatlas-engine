//! Protocol version 1 between this adapter and `packages/typescript-analyzer` (ADR-0015): the
//! request it writes to the analyzer's stdin, and the messages the analyzer writes to stdout,
//! one JSON object per line. `src/protocol/messages.ts` is the other side; change both and
//! [`PROTOCOL_VERSION`] together.

use codeatlas_application::{ProjectProblem, ResolvedFile, SemanticFailure, SemanticFailureReason};
use codeatlas_core::{
    ContentHash, DeclarationSite, ModuleReference, ModuleReferenceKind, ModuleTarget, Position,
    Range, ReferenceKind, ReferenceTarget, RepoPath, SemanticFacts, SourceLocation,
    SymbolReference, UnresolvedReason,
};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Request<'a> {
    pub protocol: u32,
    pub root: &'a str,
    pub files: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Message {
    Hello {
        protocol: u32,
    },
    Project {},
    Problem {
        path: Option<String>,
        range: Option<[u32; 4]>,
        message: String,
    },
    File {
        path: String,
        sha256: String,
        modules: Vec<WireModule>,
        references: Vec<WireReference>,
    },
    Failed {
        path: String,
        reason: WireFailure,
        message: String,
    },
    Done {
        #[serde(rename = "peakRssBytes")]
        peak_rss_bytes: u64,
    },
    Fatal {
        message: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireFailure {
    Unreadable,
    AnalyzerFailed,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireModule {
    kind: WireModuleKind,
    specifier: String,
    range: [u32; 4],
    type_only: bool,
    target: WireModuleTarget,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireModuleKind {
    Import,
    ReExport,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireModuleTarget {
    File { path: String },
    External,
    Unresolved { reason: WireReason },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireReason {
    NotFound,
    Ambiguous,
    Dynamic,
    Unsupported,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireReference {
    kind: WireReferenceKind,
    range: [u32; 4],
    target: WireReferenceTarget,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireReferenceKind {
    Call,
    Reference,
    Extends,
    Implements,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireReferenceTarget {
    Declarations {
        sites: Vec<WireSite>,
    },
    Unresolved {
        specifier: String,
        reason: WireReason,
    },
}

#[derive(Debug, Deserialize)]
struct WireSite {
    path: String,
    at: [u32; 2],
}

impl From<WireReason> for UnresolvedReason {
    fn from(reason: WireReason) -> Self {
        match reason {
            WireReason::NotFound => UnresolvedReason::NotFound,
            WireReason::Ambiguous => UnresolvedReason::Ambiguous,
            WireReason::Dynamic => UnresolvedReason::Dynamic,
            WireReason::Unsupported => UnresolvedReason::Unsupported,
        }
    }
}

fn path(p: String) -> Result<RepoPath, String> {
    RepoPath::new(p.clone()).map_err(|e| format!("{p:?} is not a repository path: {e}"))
}

fn range([sl, sc, el, ec]: [u32; 4]) -> Result<Range, String> {
    Range::new(Position::new(sl, sc), Position::new(el, ec)).map_err(|e| e.to_string())
}

fn hash(hex: &str) -> Result<ContentHash, String> {
    let bad = || format!("{hex:?} is not a SHA-256 in hexadecimal");
    if hex.len() != 64 {
        return Err(bad());
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2).ok_or_else(bad)?, 16)
            .map_err(|_| bad())?;
    }
    Ok(ContentHash::from_bytes(bytes))
}

pub(crate) fn resolved_file(
    path_text: String,
    sha256: &str,
    modules: Vec<WireModule>,
    references: Vec<WireReference>,
) -> Result<ResolvedFile, String> {
    let mut facts = SemanticFacts::default();
    for m in modules {
        facts.modules.push(ModuleReference {
            kind: match m.kind {
                WireModuleKind::Import => ModuleReferenceKind::Import,
                WireModuleKind::ReExport => ModuleReferenceKind::ReExport,
            },
            specifier: m.specifier,
            range: range(m.range)?,
            type_only: m.type_only,
            target: match m.target {
                WireModuleTarget::File { path: p } => ModuleTarget::File(path(p)?),
                WireModuleTarget::External => ModuleTarget::External,
                WireModuleTarget::Unresolved { reason } => ModuleTarget::Unresolved(reason.into()),
            },
        });
    }
    for r in references {
        facts.references.push(SymbolReference {
            kind: match r.kind {
                WireReferenceKind::Call => ReferenceKind::Call,
                WireReferenceKind::Reference => ReferenceKind::Reference,
                WireReferenceKind::Extends => ReferenceKind::Extends,
                WireReferenceKind::Implements => ReferenceKind::Implements,
            },
            range: range(r.range)?,
            target: match r.target {
                WireReferenceTarget::Declarations { sites } => ReferenceTarget::Declarations(
                    sites
                        .into_iter()
                        .map(|s| {
                            Ok(DeclarationSite {
                                path: path(s.path)?,
                                name_start: Position::new(s.at[0], s.at[1]),
                            })
                        })
                        .collect::<Result<_, String>>()?,
                ),
                WireReferenceTarget::Unresolved { specifier, reason } => {
                    ReferenceTarget::Unresolved {
                        specifier,
                        reason: reason.into(),
                    }
                }
            },
        });
    }
    Ok(ResolvedFile {
        path: path(path_text)?,
        content_hash: hash(sha256)?,
        facts,
    })
}

pub(crate) fn failure(
    path_text: String,
    reason: WireFailure,
    message: String,
) -> Result<SemanticFailure, String> {
    Ok(SemanticFailure {
        path: path(path_text)?,
        reason: match reason {
            WireFailure::Unreadable => SemanticFailureReason::Unreadable(message),
            WireFailure::AnalyzerFailed => SemanticFailureReason::AnalyzerFailed(message),
        },
    })
}

pub(crate) fn problem(
    path_text: Option<String>,
    at: Option<[u32; 4]>,
    message: String,
) -> Result<ProjectProblem, String> {
    let location = match path_text {
        Some(p) => Some(SourceLocation {
            path: path(p)?,
            range: match at {
                Some(r) => range(r)?,
                None => Range::empty(Position::new(0, 0)),
            },
        }),
        None => None,
    };
    Ok(ProjectProblem { location, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_parse_into_domain_records() {
        let line = r#"{"type":"file","path":"src/a.ts","sha256":"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
            "modules":[{"kind":"re_export","specifier":"./b","range":[0,14,0,19],"typeOnly":true,"target":{"kind":"file","path":"src/b.ts"}},
                       {"kind":"import","specifier":"name","range":[3,8,3,12],"typeOnly":false,"target":{"kind":"unresolved","reason":"dynamic"}}],
            "references":[{"kind":"call","range":[5,4,5,8],"target":{"kind":"declarations","sites":[{"path":"src/b.ts","at":[0,16]}]}},
                          {"kind":"reference","range":[6,0,6,3],"target":{"kind":"unresolved","specifier":"foo","reason":"not_found"}}],
            "futureField":1}"#;
        let Message::File {
            path,
            sha256,
            modules,
            references,
        } = serde_json::from_str(line).unwrap()
        else {
            panic!("not a file message");
        };
        let file = resolved_file(path, &sha256, modules, references).unwrap();
        assert_eq!(file.path.as_str(), "src/a.ts");
        assert_eq!(&file.content_hash.to_hex()[..8], "00112233");
        assert_eq!(file.facts.modules[0].kind, ModuleReferenceKind::ReExport);
        assert!(file.facts.modules[0].type_only);
        assert_eq!(
            file.facts.modules[1].target,
            ModuleTarget::Unresolved(UnresolvedReason::Dynamic)
        );
        assert_eq!(
            file.facts.references[0].target,
            ReferenceTarget::Declarations(vec![DeclarationSite {
                path: RepoPath::new("src/b.ts").unwrap(),
                name_start: Position::new(0, 16)
            }])
        );
        assert_eq!(file.facts.references[1].range.start(), Position::new(6, 0));
    }

    #[test]
    fn invalid_values_are_rejected() {
        assert!(hash("00").is_err());
        assert!(hash(&"zz".repeat(32)).is_err());
        assert!(range([2, 0, 1, 0]).is_err());
        assert!(path("/abs.ts".into()).is_err());
        assert!(path("a\\b.ts".into()).is_err());
        assert!(serde_json::from_str::<Message>(r#"{"type":"surprise"}"#).is_err());
    }

    #[test]
    fn the_request_uses_the_analyzer_field_names() {
        let request = Request {
            protocol: PROTOCOL_VERSION,
            root: "/repo",
            files: vec!["a.ts"],
            stack_size_mb: Some(64),
            max_memory_mb: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"protocol":1,"root":"/repo","files":["a.ts"],"stackSizeMb":64}"#
        );
    }
}
