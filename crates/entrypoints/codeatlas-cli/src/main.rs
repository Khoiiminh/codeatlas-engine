//! `codeatlas` binary: the engine's only composition root.
//! Wiring, argument parsing and output formatting only; no business rules.
//! See crates/README.md and ADR-0010.

fn main() {
    println!("codeatlas {}", env!("CARGO_PKG_VERSION"));
}
