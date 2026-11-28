//! Small helpers over Tree-sitter nodes.

use tree_sitter::Node;

/// The node's byte range, as `(start, end)`.
pub(super) fn span_of(node: Node<'_>) -> (usize, usize) {
    (node.start_byte(), node.end_byte())
}

/// The first named child of `kind`.
pub(super) fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|c| c.kind() == kind)
}

/// An anonymous token child such as `default`, `get` or `readonly`.
pub(super) fn token_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| !c.is_named() && c.kind() == kind)
}

pub(super) fn has_token(node: Node<'_>, kind: &str) -> bool {
    token_of_kind(node, kind).is_some()
}
