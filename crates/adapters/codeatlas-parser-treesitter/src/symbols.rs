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

use std::collections::BTreeSet;

use codeatlas_core::{
    Declaration, DeclaredSymbol, Language, LineIndex, SymbolKind, SymbolLimit, SymbolsTruncated,
};
use tree_sitter::{Node, TreeCursor};

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
        if x.out
            .truncated
            .is_some_and(|t| matches!(t.limit, SymbolLimit::Count { .. }))
        {
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

    fn export_statement(&mut self, node: Node<'t>, prefix: &[String]) {
        let module_scope = prefix.is_empty();
        if node.child_by_field_name("source").is_some() {
            // A re-export. Only `export * as Name from "…"` declares something here.
            let namespace = child_of_kind(node, "namespace_export");
            if let Some(name) = namespace.and_then(|n| n.named_child(0))
                && name.kind() == "identifier"
                && module_scope
            {
                let name_text = self.text_of(name).to_string();
                self.declare(
                    prefix,
                    name_text,
                    span_of(node),
                    name,
                    SymbolKind::Namespace,
                    true,
                );
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
                    let name_text = self.text_of(name).to_string();
                    self.out.exported_names.insert(name_text);
                }
            }
        } else if has_token(node, "=")
            && let Some(name) = child_of_kind(node, "identifier")
        {
            // TypeScript `export = name;`.
            let name_text = self.text_of(name).to_string();
            self.out.exported_names.insert(name_text);
        }
    }

    /// `export default <value>`: a local name, or an anonymous function or class.
    fn export_default_value(&mut self, node: Node<'t>, value: Node<'t>, prefix: &[String]) {
        let kind = match value.kind() {
            "identifier" => {
                if prefix.is_empty() {
                    let name_text = self.text_of(value).to_string();
                    self.out.exported_names.insert(name_text);
                }
                return;
            }
            "function_expression" | "generator_function" => SymbolKind::Function,
            "class" => SymbolKind::Class,
            _ => return,
        };
        let (name, name_node) = match value.child_by_field_name("name") {
            Some(n) => (self.text_of(n).to_string(), Some(n)),
            None => ("default".to_string(), token_of_kind(node, "default")),
        };
        let Some(name_node) = name_node else { return };
        let owner = self.declare(prefix, name, span_of(node), name_node, kind, true);
        if let (Some(owner), SymbolKind::Class) = (owner, kind) {
            self.members(value.child_by_field_name("body"), &owner);
        }
    }

    /// A declaration named by its `name` field (an identifier).
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
        let name_text = self.text_of(name).to_string();
        self.declare(prefix, name_text, span_of(span), name, kind, exported)
    }

    /// Records one declaration and returns its qualified name, or `None` once the symbol
    /// limit is reached.
    fn declare(
        &mut self,
        prefix: &[String],
        name: String,
        (start, end): (usize, usize),
        name_node: Node<'_>,
        kind: SymbolKind,
        exported: bool,
    ) -> Option<Vec<String>> {
        if self
            .out
            .truncated
            .is_some_and(|t| matches!(t.limit, SymbolLimit::Count { .. }))
        {
            return None;
        }
        if self.out.declared.len() >= self.max_symbols {
            self.truncate(
                start,
                SymbolLimit::Count {
                    limit: self.max_symbols,
                },
            );
            return None;
        }
        let mut qualified_name = prefix.to_vec();
        qualified_name.push(name);
        self.out.declared.push(DeclaredSymbol {
            qualified_name: qualified_name.clone(),
            declaration: Declaration {
                range: self.index.range(start, end),
                name_range: self
                    .index
                    .range(name_node.start_byte(), name_node.end_byte()),
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

    /// `const`, `let` and `var`: every bound name, including names in destructuring patterns.
    fn variables(&mut self, node: Node<'t>, prefix: &[String], exported: bool) {
        let mut cursor = node.walk();
        for declarator in node.named_children(&mut cursor) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name) = declarator.child_by_field_name("name") else {
                continue;
            };
            if name.kind() == "identifier" {
                let name_text = self.text_of(name).to_string();
                self.declare(
                    prefix,
                    name_text,
                    span_of(declarator),
                    name,
                    SymbolKind::Variable,
                    exported,
                );
                continue;
            }
            for (bound, element) in pattern_bindings(name) {
                let name_text = self.text_of(bound).to_string();
                self.declare(
                    prefix,
                    name_text,
                    span_of(element),
                    bound,
                    SymbolKind::Variable,
                    exported,
                );
            }
        }
    }

    /// `namespace A.B { … }` declares `A` and `A.B`; the body is walked as a scope.
    fn namespace(&mut self, node: Node<'t>, prefix: &[String], span: Node<'t>, exported: bool) {
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
                self.truncate(
                    segment.start_byte(),
                    SymbolLimit::NamespaceDepth {
                        limit: MAX_NAMESPACE_DEPTH,
                    },
                );
                return;
            }
            // An inner namespace of a dotted name starts at its own name.
            let start = if i == 0 {
                span.start_byte()
            } else {
                segment.start_byte()
            };
            let name_text = self.text_of(segment).to_string();
            let declared = self.declare(
                &qualified,
                name_text,
                (start, span.end_byte()),
                segment,
                SymbolKind::Namespace,
                exported && i == 0,
            );
            let Some(next) = declared else { return };
            qualified = next;
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.enter.push((body, qualified));
        }
    }

    /// Members of a class or interface body.
    fn members(&mut self, body: Option<Node<'t>>, owner: &[String]) {
        let Some(body) = body else { return };
        let in_class = body.kind() == "class_body";
        let mut cursor = body.walk();
        let children: Vec<Node<'t>> = body.children(&mut cursor).collect();
        // In the TypeScript grammars a member's decorators are its preceding siblings.
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
            let kind = match member.kind() {
                "method_definition" | "method_signature" | "abstract_method_signature" => {
                    if has_token(member, "get") || has_token(member, "set") {
                        SymbolKind::Property
                    } else {
                        SymbolKind::Method
                    }
                }
                "public_field_definition" | "field_definition" | "property_signature" => {
                    SymbolKind::Property
                }
                _ => continue,
            };
            let name = member
                .child_by_field_name("name")
                .or_else(|| member.child_by_field_name("property"));
            let Some(name) = name.filter(|n| {
                matches!(
                    n.kind(),
                    "property_identifier" | "private_property_identifier"
                )
            }) else {
                continue;
            };
            let name_text = self.text_of(name).to_string();
            if in_class && kind == SymbolKind::Method && name_text == "constructor" {
                if member.kind() == "method_definition" {
                    self.parameter_properties(member, owner);
                }
                continue;
            }
            // A body ends a method; any other member owns its trailing `;` or `,`.
            let end = if member.kind() == "method_definition" {
                member.end_byte()
            } else {
                separator_end(&children[i + 1..]).unwrap_or(member.end_byte())
            };
            if self
                .declare(owner, name_text, (start, end), name, kind, false)
                .is_none()
                && self.out.truncated.is_some()
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
            let name = parameter.child_by_field_name("pattern");
            if let (true, Some(name)) = (modified, name.filter(|n| n.kind() == "identifier")) {
                let name_text = self.text_of(name).to_string();
                self.declare(
                    owner,
                    name_text,
                    span_of(parameter),
                    name,
                    SymbolKind::Property,
                    false,
                );
            }
        }
    }

    /// CommonJS: `module.exports = name`, `module.exports = { a, b: c }`,
    /// `module.exports.x = name` and `exports.x = name` export local names.
    fn commonjs_export(&mut self, expression: Node<'t>) {
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
        let mut names = Vec::new();
        match right.kind() {
            "identifier" if whole || property => names.push(right),
            "object" if whole => {
                let mut cursor = right.walk();
                for entry in right.named_children(&mut cursor) {
                    match entry.kind() {
                        "shorthand_property_identifier" => names.push(entry),
                        "pair" => {
                            if let Some(value) = entry.child_by_field_name("value")
                                && value.kind() == "identifier"
                            {
                                names.push(value);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        for name in names {
            let name_text = self.text_of(name).to_string();
            self.out.exported_names.insert(name_text);
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
}

fn span_of(node: Node<'_>) -> (usize, usize) {
    (node.start_byte(), node.end_byte())
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|c| c.kind() == kind)
}

/// An anonymous token child such as `default`, `get` or `readonly`.
fn token_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| !c.is_named() && c.kind() == kind)
}

fn has_token(node: Node<'_>, kind: &str) -> bool {
    token_of_kind(node, kind).is_some()
}

/// End of the `;` or `,` that follows a member, skipping comments.
fn separator_end(following: &[Node<'_>]) -> Option<usize> {
    following
        .iter()
        .find(|n| n.kind() != "comment")
        .filter(|n| !n.is_named() && matches!(n.kind(), ";" | ","))
        .map(|n| n.end_byte())
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

/// Every name a destructuring pattern binds, with its element of the pattern: `b` in
/// `{ a: b }` has the element `a: b`; `x` in `[x = 1]` has `x = 1`.
fn pattern_bindings(pattern: Node<'_>) -> Vec<(Node<'_>, Node<'_>)> {
    let mut found = Vec::new();
    let mut patterns = vec![pattern];
    while let Some(pattern) = patterns.pop() {
        let mut cursor = pattern.walk();
        for element in pattern.named_children(&mut cursor) {
            match element.kind() {
                "identifier" | "shorthand_property_identifier_pattern" => {
                    found.push((element, element))
                }
                "object_pattern" | "array_pattern" => patterns.push(element),
                "pair_pattern" => {
                    let target = element.child_by_field_name("value");
                    let target = target.map(|t| match t.kind() {
                        "assignment_pattern" => t.child_by_field_name("left").unwrap_or(t),
                        _ => t,
                    });
                    match target {
                        Some(t) if t.kind() == "identifier" => found.push((t, element)),
                        Some(t) if matches!(t.kind(), "object_pattern" | "array_pattern") => {
                            patterns.push(t)
                        }
                        _ => {}
                    }
                }
                "object_assignment_pattern" | "assignment_pattern" => {
                    match element.child_by_field_name("left") {
                        Some(left)
                            if matches!(
                                left.kind(),
                                "identifier" | "shorthand_property_identifier_pattern"
                            ) =>
                        {
                            found.push((left, element));
                        }
                        Some(left) if matches!(left.kind(), "object_pattern" | "array_pattern") => {
                            patterns.push(left)
                        }
                        _ => {}
                    }
                }
                "rest_pattern" => match element.named_child(0) {
                    Some(inner) if inner.kind() == "identifier" => found.push((inner, element)),
                    Some(inner) if matches!(inner.kind(), "object_pattern" | "array_pattern") => {
                        patterns.push(inner)
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    found
}
