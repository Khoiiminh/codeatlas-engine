//! What a module exports: `export` statements, and CommonJS assignments in JavaScript.
//!
//! An `export` modifier marks the declaration it wraps. Export lists, `export default name`,
//! `export = name` and CommonJS assignments name local symbols instead; those names go to
//! `Extracted::exported_names`, and `codeatlas_core::build_symbols` applies them.

use codeatlas_core::SymbolKind;
use tree_sitter::Node;

use super::Extractor;
use super::nodes::{child_of_kind, has_token, span_of, token_of_kind};

impl<'t> Extractor<'_, 't> {
    pub(super) fn export_statement(&mut self, node: Node<'t>, prefix: &[String]) {
        let module_scope = prefix.is_empty();
        if node.child_by_field_name("source").is_some() {
            // A re-export. Only `export * as Name from "…"` declares something here.
            let namespace = child_of_kind(node, "namespace_export");
            if let Some(name) = namespace.and_then(|n| n.named_child(0))
                && name.kind() == "identifier"
                && module_scope
            {
                self.declare(prefix, name, span_of(node), SymbolKind::Namespace, true);
            }
            return;
        }
        if let Some(declaration) = node.child_by_field_name("declaration") {
            self.statement(declaration, prefix, Some(node), true);
            return;
        }
        if let Some(value) = node.child_by_field_name("value") {
            self.export_default_value(node, value, prefix);
            return;
        }
        if !module_scope {
            return;
        }
        if let Some(clause) = child_of_kind(node, "export_clause") {
            // `export { a, b as c }`: the local names a and b.
            let mut cursor = clause.walk();
            for specifier in clause.named_children(&mut cursor) {
                if let Some(name) = specifier.child_by_field_name("name")
                    && name.kind() == "identifier"
                {
                    self.export_name(name);
                }
            }
        } else if has_token(node, "=")
            && let Some(name) = child_of_kind(node, "identifier")
        {
            // TypeScript `export = name;`.
            self.export_name(name);
        }
    }

    /// `export default <value>`: a local name, or an anonymous function or class.
    fn export_default_value(&mut self, node: Node<'t>, value: Node<'t>, prefix: &[String]) {
        let kind = match value.kind() {
            "identifier" => {
                if prefix.is_empty() {
                    self.export_name(value);
                }
                return;
            }
            "function_expression" | "generator_function" => SymbolKind::Function,
            "class" => SymbolKind::Class,
            _ => return,
        };
        let owner = match value.child_by_field_name("name") {
            Some(name) => self.declare(prefix, name, span_of(node), kind, true),
            None => {
                let Some(keyword) = token_of_kind(node, "default") else {
                    return;
                };
                let name = "default".to_string();
                self.record(prefix, name, keyword, span_of(node), kind, true)
            }
        };
        if let (Some(owner), SymbolKind::Class) = (owner, kind) {
            self.members(value.child_by_field_name("body"), &owner);
        }
    }

    /// CommonJS: `module.exports = name`, `module.exports = { a, b: c }`,
    /// `module.exports.x = name` and `exports.x = name` export local names.
    pub(super) fn commonjs_export(&mut self, expression: Node<'t>) {
        if expression.kind() != "assignment_expression" {
            return;
        }
        let (Some(left), Some(right)) = (
            expression.child_by_field_name("left"),
            expression.child_by_field_name("right"),
        ) else {
            return;
        };
        let whole = self.is_module_exports(left);
        let property = left.kind() == "member_expression"
            && left.child_by_field_name("object").is_some_and(|object| {
                self.is_module_exports(object)
                    || (object.kind() == "identifier" && self.text_of(object) == "exports")
            });
        match right.kind() {
            "identifier" if whole || property => self.export_name(right),
            "object" if whole => {
                let mut cursor = right.walk();
                for entry in right.named_children(&mut cursor) {
                    let name = match entry.kind() {
                        "shorthand_property_identifier" => Some(entry),
                        "pair" => entry
                            .child_by_field_name("value")
                            .filter(|v| v.kind() == "identifier"),
                        _ => None,
                    };
                    if let Some(name) = name {
                        self.export_name(name);
                    }
                }
            }
            _ => {}
        }
    }

    fn is_module_exports(&self, node: Node<'_>) -> bool {
        node.kind() == "member_expression"
            && node
                .child_by_field_name("object")
                .is_some_and(|o| o.kind() == "identifier" && self.text_of(o) == "module")
            && node
                .child_by_field_name("property")
                .is_some_and(|p| self.text_of(p) == "exports")
    }

    /// Marks the module-scope name `name` as exported.
    fn export_name(&mut self, name: Node<'_>) {
        let text = self.text_of(name).to_string();
        self.out.exported_names.insert(text);
    }
}
