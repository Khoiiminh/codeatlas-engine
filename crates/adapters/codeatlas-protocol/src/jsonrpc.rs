//! JSON-RPC 2.0 envelopes and the CodeAtlas runtime methods (ADR-0005).
//! Each type mirrors the `$defs` entry of the same name in `contracts/schemas/codeatlas.schema.json`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::analysis::{
    AnalysisRequest, Edge, EdgeKind, Language, Node, NodeId, SnapshotId, SnapshotInfo,
};

pub use crate::analysis::AnalysisResult as AnalysisRunResult;

/// The `jsonrpc` member of every message.
pub const JSONRPC_VERSION: &str = "2.0";

/// Method names (`RequestMethod` and `NotificationMethod` in the schema).
pub mod method {
    pub const INITIALIZE: &str = "initialize";
    pub const ANALYSIS_RUN: &str = "analysis/run";
    pub const GRAPH_QUERY: &str = "graph/query";
    pub const SNAPSHOT_LIST: &str = "snapshot/list";
    pub const SHUTDOWN: &str = "shutdown";

    pub const PROGRESS: &str = "$/progress";
    pub const CANCEL_REQUEST: &str = "$/cancelRequest";
    pub const EXIT: &str = "exit";

    pub const REQUESTS: [&str; 5] = [
        INITIALIZE,
        ANALYSIS_RUN,
        GRAPH_QUERY,
        SNAPSHOT_LIST,
        SHUTDOWN,
    ];
    pub const NOTIFICATIONS: [&str; 3] = [PROGRESS, CANCEL_REQUEST, EXIT];
}

/// Error codes (`ErrorCode` in the schema; names from its `x-names`).
pub mod error_code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    pub const NOT_INITIALIZED: i64 = -32002;
    pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32010;
    pub const ANALYSIS_FAILED: i64 = -32020;
    pub const SNAPSHOT_NOT_FOUND: i64 = -32021;
    pub const REQUEST_CANCELLED: i64 = -32800;

    /// All codes with their schema names, in schema order.
    pub const ALL: [(&str, i64); 10] = [
        ("PARSE_ERROR", PARSE_ERROR),
        ("INVALID_REQUEST", INVALID_REQUEST),
        ("METHOD_NOT_FOUND", METHOD_NOT_FOUND),
        ("INVALID_PARAMS", INVALID_PARAMS),
        ("INTERNAL_ERROR", INTERNAL_ERROR),
        ("NOT_INITIALIZED", NOT_INITIALIZED),
        ("UNSUPPORTED_PROTOCOL_VERSION", UNSUPPORTED_PROTOCOL_VERSION),
        ("ANALYSIS_FAILED", ANALYSIS_FAILED),
        ("SNAPSHOT_NOT_FOUND", SNAPSHOT_NOT_FOUND),
        ("REQUEST_CANCELLED", REQUEST_CANCELLED),
    ];
}

/// Request ID: a string or an integer.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Request {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SuccessResponse {
    pub jsonrpc: String,
    pub id: RequestId,
    /// Required; `null` for methods without a result (for example `shutdown`).
    pub result: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ErrorObject {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ErrorResponse {
    pub jsonrpc: String,
    /// `None` (serialized as `null`) when the request could not be read.
    pub id: Option<RequestId>,
    pub error: ErrorObject,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub client: ClientInfo,
    /// Most preferred first.
    pub protocol_versions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ClientCapabilities>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ServerCapabilities {
    pub methods: Vec<String>,
    pub progress: bool,
    pub cancellation: bool,
    pub languages: Vec<Language>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub contract_version: String,
    pub engine: EngineInfo,
    pub capabilities: ServerCapabilities,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRunParams {
    pub request: AnalysisRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GraphQueryKind {
    FindSymbols,
    Callers,
    Callees,
    Dependencies,
    Dependents,
    Neighbors,
}

/// A bounded graph query. `find_symbols` needs `text`; every other kind needs `node_id`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphQuery {
    pub kind: GraphQueryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_kinds: Option<Vec<EdgeKind>>,
    /// 1..=5; defaults to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
    /// 1..=1000; defaults to 200.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphQueryParams {
    pub snapshot_id: SnapshotId,
    pub query: GraphQuery,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GraphQueryResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// 1..=100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SnapshotListResult {
    pub snapshots: Vec<SnapshotInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Begin,
    Report,
    End,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProgressParams {
    pub token: String,
    pub kind: ProgressKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 0..=100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CancelParams {
    pub id: RequestId,
}
