//! codeatlas-protocol: Adapter: canonical contract (wire) types and mapping to domain types.
//!
//! The types in this crate mirror the canonical JSON Schema bundle in
//! `contracts/schemas/codeatlas.schema.json` (FND-03). The schema is the source of truth;
//! `tests/contract_compatibility.rs` proves that every schema example deserializes into these
//! types and serializes back unchanged.
//!
//! Wire types never leak inward: domain and application crates do not depend on this crate
//! (ADR-0010). Mapping between wire and domain types is added here as use cases need it.
//!
//! Consumer rule: unknown JSON properties are ignored when reading, so a newer producer that
//! adds fields does not break an older reader (contracts/README.md).
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).

pub mod analysis;
pub mod jsonrpc;
pub mod version;

pub use version::{CONTRACT_VERSION, PROTOCOL_VERSION};
