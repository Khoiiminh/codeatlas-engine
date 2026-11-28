//! Members of class and interface bodies, and constructor parameter properties.
//!
//! A member's range starts at its first decorator. In the TypeScript grammars a member's
//! decorators are its preceding siblings in the body, not its children, so they are collected
//! while walking the body. A member other than a method with a body also owns the `;` or `,`
//! that ends it, as in the TypeScript compiler's tree.

use codeatlas_core::SymbolKind;
use tree_sitter::Node;

use super::Extractor;
use super::nodes::{child_of_kind, has_token, span_of};

impl<'t> Extractor<'_, 't> {
    /// Members of a class or interface body, recorded under `owner`.
    pub(super) fn members(&mut self, body: Option<Node<'t>>, owner: &[String]) {
        let Some(body) = body else { return };
        let in_class = body.kind() == "class_body";
        let mut cursor = body.walk();
        let children: Vec<Node<'t>> = body.children(&mut cursor).collect();
        let mut decorators_start: Option<usize> = None;
        for (i, &member) in children.iter().enumerate() {
            if !member.is_named() || member.kind() == "comment" {
                continue;
            }
            if member.kind() == "decorator" {
                decorators_start.get_or_insert(member.start_byte());
                continue;
            }
            let start = decorators_start
                .take()
                .unwrap_or(member.start_byte())
                .min(member.start_byte());
            let Some(kind) = member_kind(member) else {
                continue;
            };
            let name = member
                .child_by_field_name("name")
                .or_else(|| member.child_by_field_name("property"))
                .filter(|n| {
                    matches!(
                        n.kind(),
                        "property_identifier" | "private_property_identifier"
                    )
                });
            let Some(name) = name else { continue };
            if in_class && kind == SymbolKind::Method && self.text_of(name) == "constructor" {
                if member.kind() == "method_definition" {
                    self.parameter_properties(member, owner);
                }
                continue;
            }
            let end = if member.kind() == "method_definition" {
                member.end_byte()
            } else {
                separator_end(&children[i + 1..]).unwrap_or(member.end_byte())
            };
            if self
                .declare(owner, name, (start, end), kind, false)
                .is_none()
            {
                return;
            }
        }
    }

    /// `constructor(private readonly clock: Clock)` declares the property `clock`.
    fn parameter_properties(&mut self, constructor: Node<'t>, owner: &[String]) {
        let Some(parameters) = constructor.child_by_field_name("parameters") else {
            return;
        };
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            if !matches!(
                parameter.kind(),
                "required_parameter" | "optional_parameter"
            ) {
                continue;
            }
            let modified = child_of_kind(parameter, "accessibility_modifier").is_some()
                || child_of_kind(parameter, "override_modifier").is_some()
                || has_token(parameter, "readonly");
            let name = parameter
                .child_by_field_name("pattern")
                .filter(|n| n.kind() == "identifier");
            if let (true, Some(name)) = (modified, name) {
                self.declare(owner, name, span_of(parameter), SymbolKind::Property, false);
            }
        }
    }
}

/// A method, or a property (fields, property signatures, and accessors); `None` for members
/// that are not symbols (index signatures, static blocks, call signatures).
fn member_kind(member: Node<'_>) -> Option<SymbolKind> {
    match member.kind() {
        "method_definition" | "method_signature" | "abstract_method_signature" => {
            if has_token(member, "get") || has_token(member, "set") {
                Some(SymbolKind::Property)
            } else {
                Some(SymbolKind::Method)
            }
        }
        "public_field_definition" | "field_definition" | "property_signature" => {
            Some(SymbolKind::Property)
        }
        _ => None,
    }
}

/// End of the `;` or `,` that follows a member, skipping comments.
fn separator_end(following: &[Node<'_>]) -> Option<usize> {
    following
        .iter()
        .find(|n| n.kind() != "comment")
        .filter(|n| !n.is_named() && matches!(n.kind(), ";" | ","))
        .map(|n| n.end_byte())
}
