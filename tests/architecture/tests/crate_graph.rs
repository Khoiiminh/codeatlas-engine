//! Runs the boundary check against the real workspace via `cargo metadata`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use cargo_metadata::{DependencyKind, MetadataCommand, PackageId};
use codeatlas_architecture_tests::{Crate, MATRIX, check};

#[test]
fn crate_graph_matches_dependency_matrix() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let meta = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .exec()
        .expect("cargo metadata failed");

    let resolve = meta
        .resolve
        .as_ref()
        .expect("cargo metadata returned no resolve graph");
    let nodes: BTreeMap<&PackageId, _> = resolve.nodes.iter().map(|n| (&n.id, n)).collect();
    let names: BTreeMap<&PackageId, &str> = meta
        .packages
        .iter()
        .map(|p| (&p.id, p.name.as_str()))
        .collect();
    let members: BTreeSet<&PackageId> = meta.workspace_members.iter().collect();
    let ws_root = meta.workspace_root.as_std_path();

    let crates: Vec<Crate> =
        meta.workspace_packages()
            .into_iter()
            .map(|pkg| {
                let rel_dir = pkg
                    .manifest_path
                    .parent()
                    .expect("manifest has a parent")
                    .as_std_path()
                    .strip_prefix(ws_root)
                    .expect("member is inside the workspace")
                    .to_string_lossy()
                    .replace('\\', "/");

                // Direct workspace dependencies of any kind.
                let workspace_deps = nodes[&pkg.id]
                    .deps
                    .iter()
                    .filter(|d| members.contains(&d.pkg))
                    .map(|d| names[&d.pkg].to_string())
                    .collect();

                // External crates reachable through normal/build edges.
                let mut seen = BTreeSet::new();
                let mut stack = vec![&pkg.id];
                while let Some(id) = stack.pop() {
                    for d in &nodes[id].deps {
                        let runtime_edge = d.dep_kinds.iter().any(|k| {
                            matches!(k.kind, DependencyKind::Normal | DependencyKind::Build)
                        });
                        if runtime_edge && seen.insert(&d.pkg) {
                            stack.push(&d.pkg);
                        }
                    }
                }
                let transitive_external = seen
                    .into_iter()
                    .filter(|id| !members.contains(id))
                    .map(|id| names[id].to_string())
                    .collect();

                Crate {
                    name: pkg.name.to_string(),
                    rel_dir,
                    workspace_deps,
                    transitive_external,
                }
            })
            .collect();

    let product_crates = crates
        .iter()
        .filter(|c| c.rel_dir.starts_with("crates/"))
        .count();
    assert_eq!(
        product_crates,
        MATRIX.len(),
        "every crate under crates/ must be a workspace member"
    );

    let violations = check(&crates);
    assert!(
        violations.is_empty(),
        "architecture boundary violations (see crates/README.md and ADR-0010):\n  {}",
        violations.join("\n  ")
    );
}
