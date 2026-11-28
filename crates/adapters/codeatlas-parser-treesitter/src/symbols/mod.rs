//! Symbol extraction from a Tree-sitter tree (ENG-03).
//!
//! This module finds declarations and reports them as `DeclaredSymbol`s; `codeatlas-core`
//! merges them into symbols and owns the identity rules. What counts as a symbol follows
//! `tests/fixtures/README.md` ("Symbols"), extended in `tests/symbols/README.md`:
//!
//! - module scope: functions (with overload signatures), classes, interfaces, type aliases,
//!   enums, variables (each name bound by a destructuring pattern too), `export * as Name`
//!   namespaces, and `namespace`/`module` declarations, whose bodies are a scope like the
//!   module's;
//! - class members: methods, properties, accessors (as properties) and constructor parameter
//!   properties; interface members: method and property signatures;
//! - anonymous default exports (`export default function () {}`) are named `default`.
//!
//! Members named by a string, a number or a computed expression are skipped, as are
//! constructors, index signatures, static blocks, enum members, locals, parameters, object
//! literal members, class expressions and `declare module "x"` / `declare global` blocks.
//!
//! The walk uses explicit stacks, not recursion, so deeply nested input cannot overflow the
//! stack. Namespaces nested more than [`MAX_NAMESPACE_DEPTH`] levels are not extracted.
//!
//! | File | Handles |
//! |---|---|
//! | `mod.rs` | The walk over scopes, one statement at a time, and recording declarations |
//! | `exports.rs` | `export` statements and CommonJS exports |
//! | `members.rs` | Class and interface members, constructor parameter properties |
//! | `variables.rs` | `const`, `let` and `var`, including destructuring patterns |
//! | `namespaces.rs` | `namespace A.B { … }` |
//! | `nodes.rs` | Small Tree-sitter helpers |

mod exports;
mod members;
mod namespaces;
mod nodes;
mod variables;

use std::collections::BTreeSet;

use codeatlas_core::{
    Declaration, DeclaredSymbol, Language, LineIndex, SymbolKind, SymbolLimit, SymbolsTruncated,
};
use tree_sitter::{Node, TreeCursor};

use nodes::span_of;

/// Deepest namespace nesting that is extracted. Real code nests two or three levels; the
/// limit stops qualified names from growing without bound on generated input.
pub const MAX_NAMESPACE_DEPTH: usize = 32;

/// What one file declares, before declarations are merged into symbols.
pub(crate) struct Extracted {
    pub declared: Vec<DeclaredSymbol>,
    /// Module-scope names exported by an export list rather than an `export` modifier.
    pub exported_names: BTreeSet<String>,
    pub truncated: Option<SymbolsTruncated>,
}

/// A statement list being walked: the module, or a namespace body.
struct Scope<'t> {
    cursor: TreeCursor<'t>,
    entered: bool,
    /// Qualified name of the namespace, empty at module scope.
    prefix: Vec<String>,
}

pub(crate) fn extract(
    root: Node<'_>,
    text: &str,
    language: Language,
    index: &LineIndex<'_>,
    max_symbols: usize,
) -> Extracted {
    let mut x = Extractor {
        text,
        index,
        commonjs: matches!(language, Language::JavaScript | Language::Jsx),
        max_symbols,
        out: Extracted {
            declared: Vec::new(),
            exported_names: BTreeSet::new(),
            truncated: None,
        },
        enter: Vec::new(),
    };
    let mut scopes = vec![Scope {
        cursor: root.walk(),
        entered: false,
        prefix: Vec::new(),
    }];
    while let Some(scope) = scopes.last_mut() {
        let moved = if scope.entered {
            scope.cursor.goto_next_sibling()
        } else {
            scope.entered = true;
            scope.cursor.goto_first_child()
        };
        if !moved {
            scopes.pop();
            continue;
        }
        let node = scope.cursor.node();
        if node.is_named() {
            x.statement(node, &scope.prefix, None, false);
        }
        if x.stopped() {
            break;
        }
        // Namespace bodies, and statements an error node wraps, are walked next, so
        // declarations are visited in document order.
        for (body, prefix) in x.enter.drain(..).rev() {
            scopes.push(Scope {
                cursor: body.walk(),
                entered: false,
                prefix,
            });
        }
    }
    x.out
}

struct Extractor<'a, 't> {
    text: &'a str,
    index: &'a LineIndex<'a>,
    /// CommonJS exports count (JavaScript and JSX files).
    commonjs: bool,
    max_symbols: usize,
    out: Extracted,
    /// Statement lists to walk after the current statement.
    enter: Vec<(Node<'t>, Vec<String>)>,
}

impl<'t> Extractor<'_, 't> {
    fn text_of(&self, node: Node<'_>) -> &str {
        self.text.get(node.byte_range()).unwrap_or_default()
    }

    /// One statement in a scope. `outer` is the outermost wrapper (`export …`, `declare …`),
    /// whose range the declaration takes.
    fn statement(
        &mut self,
        node: Node<'t>,
        prefix: &[String],
        outer: Option<Node<'t>>,
        exported: bool,
    ) {
        let span = outer.unwrap_or(node);
        match node.kind() {
            "export_statement" => self.export_statement(node, prefix),
            "ambient_declaration" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    // A `statement_block` child is `declare global { … }`.
                    if child.kind() != "statement_block" {
                        self.statement(child, prefix, Some(span), exported);
                    }
                }
            }
            "expression_statement" => match node.named_child(0) {
                // `namespace A.B { … }` parses as an expression statement.
                Some(inner) if inner.kind() == "internal_module" => {
                    self.namespace(inner, prefix, outer.unwrap_or(inner), exported);
                }
                Some(inner) if self.commonjs && prefix.is_empty() => self.commonjs_export(inner),
                _ => {}
            },
            "function_declaration" | "generator_function_declaration" | "function_signature" => {
                self.named(node, prefix, span, SymbolKind::Function, exported);
            }
            "class_declaration" | "abstract_class_declaration" => {
                if let Some(owner) = self.named(node, prefix, span, SymbolKind::Class, exported) {
                    self.members(node.child_by_field_name("body"), &owner);
                }
            }
            "interface_declaration" => {
                if let Some(owner) = self.named(node, prefix, span, SymbolKind::Interface, exported)
                {
                    self.members(node.child_by_field_name("body"), &owner);
                }
            }
            "type_alias_declaration" => {
                self.named(node, prefix, span, SymbolKind::TypeAlias, exported);
            }
            "enum_declaration" => {
                self.named(node, prefix, span, SymbolKind::Enum, exported);
            }
            "lexical_declaration" | "variable_declaration" => {
                self.variables(node, prefix, exported)
            }
            "internal_module" | "module" => self.namespace(node, prefix, span, exported),
            // Parser recovery can wrap whole declarations in an error node; look inside.
            "ERROR" => self.enter.push((node, prefix.to_vec())),
            _ => {}
        }
    }

    /// A declaration named by its `name` field (an identifier). Returns its qualified name.
    fn named(
        &mut self,
        node: Node<'t>,
        prefix: &[String],
        span: Node<'t>,
        kind: SymbolKind,
        exported: bool,
    ) -> Option<Vec<String>> {
        let name = node.child_by_field_name("name")?;
        if !matches!(name.kind(), "identifier" | "type_identifier") {
            return None;
        }
        self.declare(prefix, name, span_of(span), kind, exported)
    }

    /// Records a declaration whose name is the text of `name`. Returns its qualified name, or
    /// `None` once the symbol limit is reached.
    fn declare(
        &mut self,
        prefix: &[String],
        name: Node<'_>,
        span: (usize, usize),
        kind: SymbolKind,
        exported: bool,
    ) -> Option<Vec<String>> {
        let text = self.text_of(name).to_string();
        self.record(prefix, text, name, span, kind, exported)
    }

    /// Records a declaration named `name`, whose name range is `name_node`. Only an anonymous
    /// default export needs a name that differs from the node's text.
    fn record(
        &mut self,
        prefix: &[String],
        name: String,
        name_node: Node<'_>,
        (start, end): (usize, usize),
        kind: SymbolKind,
        exported: bool,
    ) -> Option<Vec<String>> {
        if self.stopped() {
            return None;
        }
        if self.out.declared.len() >= self.max_symbols {
            let limit = SymbolLimit::Count {
                limit: self.max_symbols,
            };
            self.truncate(start, limit);
            return None;
        }
        let mut qualified_name = prefix.to_vec();
        qualified_name.push(name);
        let name_range = self
            .index
            .range(name_node.start_byte(), name_node.end_byte());
        self.out.declared.push(DeclaredSymbol {
            qualified_name: qualified_name.clone(),
            declaration: Declaration {
                range: self.index.range(start, end),
                name_range,
                kind,
            },
            exported: exported && prefix.is_empty(),
        });
        Some(qualified_name)
    }

    /// Keeps the first place where extraction stopped.
    fn truncate(&mut self, at_byte: usize, limit: SymbolLimit) {
        if self.out.truncated.is_none() {
            self.out.truncated = Some(SymbolsTruncated {
                at: self.index.position(at_byte),
                limit,
            });
        }
    }

    /// The symbol limit was reached: nothing more is recorded.
    fn stopped(&self) -> bool {
        self.out
            .truncated
            .is_some_and(|t| matches!(t.limit, SymbolLimit::Count { .. }))
    }
}
