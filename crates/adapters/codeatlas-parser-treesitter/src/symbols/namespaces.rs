//! `namespace A.B { … }` declares `A` and `A.B`; the body is walked as a scope like the
//! module's. The inner namespace of a dotted name starts at its own name, as in the TypeScript
//! compiler's tree. Namespaces nested more than `MAX_NAMESPACE_DEPTH` levels stop extraction
//! there, reported as a symbol limit.

use codeatlas_core::{SymbolKind, SymbolLimit};
use tree_sitter::Node;

use super::{Extractor, MAX_NAMESPACE_DEPTH};

impl<'t> Extractor<'_, 't> {
    pub(super) fn namespace(
        &mut self,
        node: Node<'t>,
        prefix: &[String],
        span: Node<'t>,
        exported: bool,
    ) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        // A string name is an ambient module (`declare module "x"`), not a namespace.
        let Some(segments) = dotted_name(name) else {
            return;
        };
        let mut qualified = prefix.to_vec();
        for (i, segment) in segments.into_iter().enumerate() {
            if qualified.len() >= MAX_NAMESPACE_DEPTH {
                let limit = SymbolLimit::NamespaceDepth {
                    limit: MAX_NAMESPACE_DEPTH,
                };
                self.truncate(segment.start_byte(), limit);
                return;
            }
            let start = if i == 0 {
                span.start_byte()
            } else {
                segment.start_byte()
            };
            let range = (start, span.end_byte());
            let outermost = exported && i == 0;
            let declared =
                self.declare(&qualified, segment, range, SymbolKind::Namespace, outermost);
            let Some(next) = declared else { return };
            qualified = next;
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.enter.push((body, qualified));
        }
    }
}

/// The segments of `A` or `A.B.C`, outermost first. `None` for a string name.
fn dotted_name(name: Node<'_>) -> Option<Vec<Node<'_>>> {
    let mut segments = Vec::new();
    let mut current = name;
    loop {
        match current.kind() {
            "identifier" => {
                segments.push(current);
                break;
            }
            // The grammar writes the inner part of `A.B.C` as a member expression.
            "nested_identifier" | "member_expression" => {
                segments.push(current.child_by_field_name("property")?);
                current = current.child_by_field_name("object")?;
            }
            _ => return None,
        }
    }
    segments.reverse();
    Some(segments)
}
