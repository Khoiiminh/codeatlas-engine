//! `const`, `let` and `var`: every bound name, including names in destructuring patterns.
//!
//! A plain variable's range is its declarator (`a = 1`); a destructured name's range is its
//! element of the pattern (`second: renamed`, `...rest`, `x = 2`), as in the TypeScript
//! compiler's tree.

use codeatlas_core::SymbolKind;
use tree_sitter::Node;

use super::Extractor;
use super::nodes::span_of;

impl<'t> Extractor<'_, 't> {
    pub(super) fn variables(&mut self, node: Node<'t>, prefix: &[String], exported: bool) {
        let mut cursor = node.walk();
        for declarator in node.named_children(&mut cursor) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name) = declarator.child_by_field_name("name") else {
                continue;
            };
            if name.kind() == "identifier" {
                let span = span_of(declarator);
                self.declare(prefix, name, span, SymbolKind::Variable, exported);
                continue;
            }
            for (bound, element) in pattern_bindings(name) {
                let span = span_of(element);
                self.declare(prefix, bound, span, SymbolKind::Variable, exported);
            }
        }
    }
}

/// Every name a destructuring pattern binds, with its element of the pattern: `b` in
/// `{ a: b }` has the element `a: b`; `x` in `[x = 1]` has `x = 1`. Nested patterns go on a
/// work list instead of a recursive call, so any depth is safe.
fn pattern_bindings(pattern: Node<'_>) -> Vec<(Node<'_>, Node<'_>)> {
    let is_pattern = |n: &Node<'_>| matches!(n.kind(), "object_pattern" | "array_pattern");
    let mut found = Vec::new();
    let mut patterns = vec![pattern];
    while let Some(pattern) = patterns.pop() {
        let mut cursor = pattern.walk();
        for element in pattern.named_children(&mut cursor) {
            // The part of the element that binds: a name, or a nested pattern.
            let target = match element.kind() {
                "identifier" | "shorthand_property_identifier_pattern" => Some(element),
                "object_pattern" | "array_pattern" => Some(element),
                // `a: b`, or `a: b = 1`
                "pair_pattern" => element.child_by_field_name("value").map(|v| {
                    if v.kind() == "assignment_pattern" {
                        v.child_by_field_name("left").unwrap_or(v)
                    } else {
                        v
                    }
                }),
                // `{ a = 1 }`, `[a = 1]`
                "object_assignment_pattern" | "assignment_pattern" => {
                    element.child_by_field_name("left")
                }
                // `...rest`
                "rest_pattern" => element.named_child(0),
                _ => None,
            };
            match target {
                Some(t) if is_pattern(&t) => patterns.push(t),
                Some(t)
                    if matches!(
                        t.kind(),
                        "identifier" | "shorthand_property_identifier_pattern"
                    ) =>
                {
                    found.push((t, element));
                }
                _ => {}
            }
        }
    }
    found
}
