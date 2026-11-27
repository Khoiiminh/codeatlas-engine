//! FND-03 contract tests: the canonical schema, its examples and the Rust wire types agree.
//!
//! 1. The schema bundle is a valid JSON Schema (draft 2020-12).
//! 2. Every example under `contracts/fixtures/examples/<Type>/` validates (`valid-*.json`) or
//!    fails (`invalid-*.json`) against `$defs/<Type>`, with format checks on.
//! 3. Every valid example deserializes into the Rust type of the same name and serializes back
//!    to exactly the same JSON, so Rust neither drops nor invents fields.
//! 4. Versions, method names and error codes in Rust match the schema.

use std::fs;
use std::path::{Path, PathBuf};

use codeatlas_protocol::analysis::{
    AnalysisRequest, AnalysisResult, Diagnostic, Edge, Evidence, Node, Snapshot, SnapshotInfo,
};
use codeatlas_protocol::jsonrpc::{
    AnalysisRunParams, CancelParams, ErrorResponse, GraphQueryParams, GraphQueryResult,
    InitializeParams, InitializeResult, Notification, ProgressParams, Request, SnapshotListParams,
    SnapshotListResult, SuccessResponse, error_code, method,
};
use codeatlas_protocol::{CONTRACT_VERSION, PROTOCOL_VERSION};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

type RoundTrip = fn(&Value) -> Result<Value, String>;

/// Schema `$defs` name -> Rust type. Every entry must have valid and invalid examples.
const ROUND_TRIP: &[(&str, RoundTrip)] = &[
    ("Node", round_trip::<Node>),
    ("Evidence", round_trip::<Evidence>),
    ("Edge", round_trip::<Edge>),
    ("Diagnostic", round_trip::<Diagnostic>),
    ("SnapshotInfo", round_trip::<SnapshotInfo>),
    ("Snapshot", round_trip::<Snapshot>),
    ("AnalysisRequest", round_trip::<AnalysisRequest>),
    ("AnalysisResult", round_trip::<AnalysisResult>),
    ("JsonRpcRequest", round_trip::<Request>),
    ("JsonRpcNotification", round_trip::<Notification>),
    ("JsonRpcSuccessResponse", round_trip::<SuccessResponse>),
    ("JsonRpcErrorResponse", round_trip::<ErrorResponse>),
    ("InitializeParams", round_trip::<InitializeParams>),
    ("InitializeResult", round_trip::<InitializeResult>),
    ("AnalysisRunParams", round_trip::<AnalysisRunParams>),
    ("GraphQueryParams", round_trip::<GraphQueryParams>),
    ("GraphQueryResult", round_trip::<GraphQueryResult>),
    ("SnapshotListParams", round_trip::<SnapshotListParams>),
    ("SnapshotListResult", round_trip::<SnapshotListResult>),
    ("ProgressParams", round_trip::<ProgressParams>),
    ("CancelParams", round_trip::<CancelParams>),
];

fn round_trip<T: DeserializeOwned + Serialize>(value: &Value) -> Result<Value, String> {
    let typed: T =
        serde_json::from_value(value.clone()).map_err(|e| format!("deserialize: {e}"))?;
    serde_json::to_value(&typed).map_err(|e| format!("serialize: {e}"))
}

fn contracts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../contracts")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn bundle() -> Value {
    read_json(&contracts_dir().join("schemas/codeatlas.schema.json"))
}

/// A validator for one named definition: the bundle with a root `$ref` to `#/$defs/<name>`.
fn validator_for(bundle: &Value, name: &str) -> jsonschema::Validator {
    assert!(
        bundle["$defs"].get(name).is_some(),
        "schema has no $defs/{name}"
    );
    let mut schema = bundle.clone();
    schema["$ref"] = Value::String(format!("#/$defs/{name}"));
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap_or_else(|e| panic!("compile $defs/{name}: {e}"))
}

struct Example {
    path: PathBuf,
    valid: bool,
    value: Value,
}

fn examples(type_name: &str) -> Vec<Example> {
    let dir = contracts_dir().join("fixtures/examples").join(type_name);
    let mut out: Vec<Example> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|path| {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let valid = if file.starts_with("valid-") {
                true
            } else if file.starts_with("invalid-") {
                false
            } else {
                panic!(
                    "{}: example names start with valid- or invalid-",
                    path.display()
                );
            };
            Example {
                value: read_json(&path),
                path,
                valid,
            }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[test]
fn schema_bundle_is_valid_draft_2020_12() {
    let bundle = bundle();
    assert_eq!(
        bundle["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    jsonschema::meta::validate(&bundle)
        .unwrap_or_else(|e| panic!("schema is not a valid JSON Schema: {e}"));
}

#[test]
fn versions_match_schema() {
    let bundle = bundle();
    assert_eq!(bundle["x-contract-version"], CONTRACT_VERSION);
    assert_eq!(bundle["x-protocol-version"], PROTOCOL_VERSION);
    assert_eq!(
        bundle["$id"],
        format!("urn:codeatlas:contracts:{CONTRACT_VERSION}")
    );
}

#[test]
fn method_names_match_schema() {
    let bundle = bundle();
    let names = |def: &str| -> Vec<String> {
        bundle["$defs"][def]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(names("RequestMethod"), method::REQUESTS);
    assert_eq!(names("NotificationMethod"), method::NOTIFICATIONS);
}

#[test]
fn error_codes_match_schema() {
    let def = &bundle()["$defs"]["ErrorCode"];
    let codes: Vec<i64> = def["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let names: Vec<&str> = def["x-names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let rust: Vec<(&str, i64)> = error_code::ALL.to_vec();
    assert_eq!(names.into_iter().zip(codes).collect::<Vec<_>>(), rust);
}

#[test]
fn every_example_folder_is_a_round_trip_type() {
    let dir = contracts_dir().join("fixtures/examples");
    for entry in fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            ROUND_TRIP.iter().any(|(t, _)| *t == name),
            "examples/{name} has no Rust type in ROUND_TRIP; add one or rename the folder"
        );
    }
}

#[test]
fn examples_validate_against_schema() {
    let bundle = bundle();
    let mut failures = Vec::new();
    for (type_name, _) in ROUND_TRIP {
        let validator = validator_for(&bundle, type_name);
        let examples = examples(type_name);
        assert!(
            examples.iter().any(|e| e.valid),
            "{type_name}: needs at least one valid- example"
        );
        assert!(
            examples.iter().any(|e| !e.valid),
            "{type_name}: needs at least one invalid- example"
        );
        for example in examples {
            let ok = validator.is_valid(&example.value);
            if ok != example.valid {
                let detail: Vec<String> = validator
                    .iter_errors(&example.value)
                    .map(|e| e.to_string())
                    .collect();
                failures.push(format!(
                    "{} should be {} but is {} {detail:?}",
                    example.path.display(),
                    if example.valid { "valid" } else { "invalid" },
                    if ok { "valid" } else { "invalid" },
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "schema/example mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rust_types_round_trip_valid_examples() {
    let mut failures = Vec::new();
    for (type_name, round_trip) in ROUND_TRIP {
        for example in examples(type_name).into_iter().filter(|e| e.valid) {
            match round_trip(&example.value) {
                Ok(back) if back == example.value => {}
                Ok(back) => failures.push(format!(
                    "{}: round trip changed the JSON to {back}",
                    example.path.display()
                )),
                Err(e) => failures.push(format!("{}: {e}", example.path.display())),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Rust/schema mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn readers_ignore_unknown_properties() {
    // Consumer rule (contracts/README.md): a newer producer may add fields.
    let mut value = read_json(&contracts_dir().join("fixtures/examples/Node/valid-function.json"));
    value["addedInALaterMinor"] = Value::Bool(true);
    let node: Node = serde_json::from_value(value).expect("unknown property must be ignored");
    assert_eq!(node.name, "start");
}
