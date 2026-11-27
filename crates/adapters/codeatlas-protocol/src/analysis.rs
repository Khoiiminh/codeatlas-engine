//! Analysis model wire types: graph nodes and edges, evidence, snapshots, requests and results.
//! Each type mirrors the `$defs` entry of the same name in `contracts/schemas/codeatlas.schema.json`.

use serde::{Deserialize, Serialize};

/// Stable, opaque node identifier.
pub type NodeId = String;
/// Stable, opaque edge identifier.
pub type EdgeId = String;
/// Opaque snapshot identifier.
pub type SnapshotId = String;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based column in UTF-16 code units.
    pub character: u32,
}

/// Half-open range `[start, end)`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Repository-relative path with forward slashes.
    pub path: String,
    pub range: Range,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Typescript,
    Tsx,
    Javascript,
    Jsx,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File,
    ExternalModule,
    Namespace,
    Class,
    Interface,
    TypeAlias,
    Enum,
    Function,
    Method,
    Property,
    Variable,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported: Option<bool>,
    /// SHA-256 of the file content; `file` nodes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Imports,
    Exports,
    Calls,
    References,
    Extends,
    Implements,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Syntactic,
    Semantic,
    Inferred,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub kind: EvidenceKind,
    /// Producer of the evidence, for example `tree-sitter` or `typescript`.
    pub analyzer: String,
    pub location: SourceLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Resolved,
    Unresolved,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    NotFound,
    Ambiguous,
    Dynamic,
    Unsupported,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedTarget {
    pub specifier: String,
    pub reason: UnresolvedReason,
}

/// A directed relationship. The schema requires `target` exactly when `resolution` is
/// `resolved`, and `unresolved` exactly when it is `unresolved`; use [`Edge::resolved`] and
/// [`Edge::unresolved`] to build edges that satisfy it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub id: EdgeId,
    pub kind: EdgeKind,
    pub source: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<NodeId>,
    pub resolution: Resolution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<UnresolvedTarget>,
    /// At least one item.
    pub evidence: Vec<Evidence>,
}

impl Edge {
    pub fn resolved(
        id: EdgeId,
        kind: EdgeKind,
        source: NodeId,
        target: NodeId,
        evidence: Vec<Evidence>,
    ) -> Self {
        Self {
            id,
            kind,
            source,
            target: Some(target),
            resolution: Resolution::Resolved,
            unresolved: None,
            evidence,
        }
    }

    pub fn unresolved(
        id: EdgeId,
        kind: EdgeKind,
        source: NodeId,
        unresolved: UnresolvedTarget,
        evidence: Vec<Evidence>,
    ) -> Self {
        Self {
            id,
            kind,
            source,
            target: None,
            resolution: Resolution::Unresolved,
            unresolved: Some(unresolved),
            evidence,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    /// `CA` followed by four digits.
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIdentity {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceFingerprint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub content_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotStatus {
    InProgress,
    Complete,
    Failed,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStats {
    pub files: u64,
    pub nodes: u64,
    pub edges: u64,
    pub unresolved_edges: u64,
    pub diagnostics: u64,
}

/// Snapshot identity and metadata (Blueprint v1.3 section 4.1, ADR-0003).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub snapshot_id: SnapshotId,
    pub repository: RepositoryIdentity,
    pub source: SourceFingerprint,
    pub contract_version: String,
    pub engine_version: String,
    pub config_fingerprint: String,
    /// RFC 3339 timestamp.
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub status: SnapshotStatus,
    pub stats: SnapshotStats,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub info: SnapshotInfo,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<Language>>,
    /// Defaults to `true` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_analysis: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AnalysisRequest {
    /// Absolute path of the repository root on the machine running the engine.
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<AnalysisOptions>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    SnapshotGraph,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactReference {
    pub kind: ArtifactKind,
    pub uri: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub snapshot: SnapshotInfo,
    /// At most 1000 items; see `diagnostics_truncated`.
    pub diagnostics: Vec<Diagnostic>,
    pub diagnostics_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<ArtifactReference>>,
}
