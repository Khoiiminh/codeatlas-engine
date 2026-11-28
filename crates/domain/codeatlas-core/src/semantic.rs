//! What a semantic resolver found in a file, and how it becomes relationships (ENG-04).
//!
//! A semantic resolver (the TypeScript compiler, ADR-0004) reports [`SemanticFacts`] for each
//! file: the modules the file names ([`ModuleReference`]) and the declarations its calls,
//! references and heritage clauses reach ([`SymbolReference`]). It reports **places, not ids**:
//! a reached declaration is given as the file and position of its name ([`DeclarationSite`]).
//! [`link`] turns the facts into [`Relationship`]s between graph ids, using the symbols the
//! parser found (ENG-03), so symbol identity has one owner ([`crate::symbol`]) whichever
//! analyser reports a relationship.
//!
//! Rules that [`link`] applies:
//!
//! - **Sources.** An import or re-export comes from its file. A call, reference or heritage
//!   clause comes from the innermost symbol whose declaration contains it, or from the file
//!   for top-level code. Declarations include their decorators, so a reference inside
//!   `@Inject(TOKEN)` on a constructor parameter property comes from that property.
//! - **Targets.** A reached declaration becomes the symbol declared at that name position;
//!   when several declarations are given (a merged declaration), the first one that is a
//!   symbol wins. A reference whose declarations are not symbols (a local variable, an enum
//!   member, an object-literal member, a declaration file) is not a relationship and is
//!   dropped. Unresolved references are kept, with their reason: they are never dropped or
//!   guessed (ADR-0004).
//! - **Order.** Relationships are sorted by position in the file, so the result does not
//!   depend on the order in which an analyser reported them.

use std::collections::BTreeMap;
use std::fmt;

use crate::source::RepoPath;
use crate::symbol::{Symbol, SymbolId};
use crate::text::{Position, Range, SourceLocation};

/// Why a relationship has no target. The wire values are the contract's `UnresolvedReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnresolvedReason {
    /// A missing module, an undeclared name, or a name imported from a missing module.
    NotFound,
    /// The name is exported by more than one `export *` source, so no single declaration is
    /// the target (TypeScript reports TS2308 and silently picks one).
    Ambiguous,
    /// The module specifier is computed at run time.
    Dynamic,
    /// The target exists but is not a source file the analysis covers: a JSON or other
    /// non-source module, a declaration file, or a file outside the analysed source.
    Unsupported,
}

impl UnresolvedReason {
    pub const ALL: [UnresolvedReason; 4] = [
        UnresolvedReason::NotFound,
        UnresolvedReason::Ambiguous,
        UnresolvedReason::Dynamic,
        UnresolvedReason::Unsupported,
    ];

    /// The contract's wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            UnresolvedReason::NotFound => "not_found",
            UnresolvedReason::Ambiguous => "ambiguous",
            UnresolvedReason::Dynamic => "dynamic",
            UnresolvedReason::Unsupported => "unsupported",
        }
    }
}

impl fmt::Display for UnresolvedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a file names a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleReferenceKind {
    /// `import`, `import … = require(…)`, `require(…)`, `import(…)` or a type `import(…)`.
    Import,
    /// `export … from` and `export * from`: the file re-exports the module.
    ReExport,
}

/// Where a module specifier leads.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleTarget {
    /// A source file of the analysed repository.
    File(RepoPath),
    /// A package or a runtime built-in (`react`, `node:http`), named by the specifier.
    External,
    Unresolved(UnresolvedReason),
}

/// A module the file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleReference {
    pub kind: ModuleReferenceKind,
    /// The specifier as written, without quotes. For a computed specifier, the expression's
    /// source text.
    pub specifier: String,
    /// The specifier in the file, with its quotes; for a computed specifier, the expression.
    pub range: Range,
    /// `import type` or `export type … from`: the reference is erased at run time.
    pub type_only: bool,
    pub target: ModuleTarget,
}

/// What a name in the file does with the declaration it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceKind {
    /// A call, `new`, `super(…)`, a tagged template or a decorator.
    Call,
    /// A use as a value without a call, or a JSX element.
    Reference,
    /// `class A extends B`, `interface A extends B`.
    Extends,
    /// `class A implements B`.
    Implements,
}

/// A declaration, by the file and the start of its name: the identifier, or the `default`
/// keyword of an anonymous default export, in the units of [`crate::text`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclarationSite {
    pub path: RepoPath,
    pub name_start: Position,
}

/// What a name reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceTarget {
    /// The declarations of the reached entity, most specific first. Usually one; several for
    /// a merged declaration.
    Declarations(Vec<DeclarationSite>),
    /// Nothing; `specifier` is the name or expression as written.
    Unresolved {
        specifier: String,
        reason: UnresolvedReason,
    },
}

/// A call, reference or heritage clause in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolReference {
    pub kind: ReferenceKind,
    /// The name that was resolved: `join` in `room.join()`, `PartyRoom` in `new PartyRoom()`,
    /// `super` in `super()`, `Badge` in `<Ui.Badge />`.
    pub range: Range,
    pub target: ReferenceTarget,
}

/// Everything a semantic resolver found in one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticFacts {
    pub modules: Vec<ModuleReference>,
    pub references: Vec<SymbolReference>,
}

/// The kind of a relationship. The wire values are the contract's `EdgeKind` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationKind {
    Imports,
    Exports,
    Calls,
    References,
    Extends,
    Implements,
}

impl RelationKind {
    /// The contract's wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            RelationKind::Imports => "imports",
            RelationKind::Exports => "exports",
            RelationKind::Calls => "calls",
            RelationKind::References => "references",
            RelationKind::Extends => "extends",
            RelationKind::Implements => "implements",
        }
    }
}

impl fmt::Display for RelationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One end of a relationship: a graph node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Endpoint {
    File(RepoPath),
    Symbol(SymbolId),
    /// An external module, by its specifier.
    External(String),
}

impl Endpoint {
    /// The node id, as in the contract examples and the FND-04 golden files: `file:<path>`,
    /// `symbol:<path>#<qualifiedName>` or `external:<specifier>`.
    pub fn id(&self) -> String {
        match self {
            Endpoint::File(path) => format!("file:{path}"),
            Endpoint::Symbol(id) => id.as_str().to_string(),
            Endpoint::External(specifier) => format!("external:{specifier}"),
        }
    }
}

/// Where a relationship leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationTarget {
    Resolved(Endpoint),
    Unresolved {
        specifier: String,
        reason: UnresolvedReason,
    },
}

/// A relationship found by semantic analysis, before ENG-05 merges it with other evidence
/// into a graph edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub kind: RelationKind,
    pub from: Endpoint,
    pub target: RelationTarget,
    /// The specifier or name in the source file.
    pub location: SourceLocation,
    pub type_only: bool,
}

/// The symbols of a set of files, indexed for [`link`].
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    files: BTreeMap<RepoPath, FileSymbols>,
}

#[derive(Debug, Clone, Default)]
struct FileSymbols {
    /// Declaration name start → symbol. The first symbol wins if two share a position.
    by_name: BTreeMap<Position, SymbolId>,
    /// Every declaration, sorted by start and then by end, outer first.
    declarations: Vec<(Range, SymbolId)>,
    /// For each declaration, the index of the nearest earlier declaration that contains it.
    parents: Vec<Option<usize>>,
}

impl SymbolTable {
    pub fn new<'a>(symbols: impl IntoIterator<Item = &'a Symbol>) -> Self {
        let mut files: BTreeMap<RepoPath, FileSymbols> = BTreeMap::new();
        for symbol in symbols {
            let file = files.entry(symbol.path().clone()).or_default();
            for declaration in symbol.declarations() {
                file.by_name
                    .entry(declaration.name_range.start())
                    .or_insert_with(|| symbol.id().clone());
                file.declarations
                    .push((declaration.range, symbol.id().clone()));
            }
        }
        for file in files.values_mut() {
            file.declarations.sort_by(|(a, a_id), (b, b_id)| {
                (a.start(), std::cmp::Reverse(a.end()), a_id).cmp(&(
                    b.start(),
                    std::cmp::Reverse(b.end()),
                    b_id,
                ))
            });
            // Declarations nest (members inside classes, classes inside namespaces), so one
            // sweep with a stack of open declarations finds each one's parent.
            let mut open: Vec<usize> = Vec::new();
            file.parents = Vec::with_capacity(file.declarations.len());
            for (i, (range, _)) in file.declarations.iter().enumerate() {
                while let Some(&top) = open.last() {
                    if contains(&file.declarations[top].0, range.start()) {
                        break;
                    }
                    open.pop();
                }
                file.parents.push(open.last().copied());
                open.push(i);
            }
        }
        SymbolTable { files }
    }

    /// The symbol one of whose declarations has its name starting at `name_start`.
    pub fn declared_at(&self, path: &RepoPath, name_start: Position) -> Option<&SymbolId> {
        self.files.get(path)?.by_name.get(&name_start)
    }

    /// The innermost symbol whose declaration contains `at` (start inclusive, end exclusive).
    pub fn enclosing(&self, path: &RepoPath, at: Position) -> Option<&SymbolId> {
        let file = self.files.get(path)?;
        // The last declaration starting at or before `at`; if it does not contain `at`, one of
        // the declarations around it may.
        let after = file
            .declarations
            .partition_point(|(range, _)| range.start() <= at);
        let mut candidate = after.checked_sub(1);
        while let Some(i) = candidate {
            let (range, id) = &file.declarations[i];
            if contains(range, at) {
                return Some(id);
            }
            candidate = file.parents[i];
        }
        None
    }
}

fn contains(range: &Range, at: Position) -> bool {
    range.start() <= at && at < range.end()
}

/// The relationships of the file at `path`, from its semantic facts and the symbol table of
/// the analysed files. See the module documentation for the rules.
pub fn link(path: &RepoPath, facts: &SemanticFacts, symbols: &SymbolTable) -> Vec<Relationship> {
    let at = |range: Range| SourceLocation {
        path: path.clone(),
        range,
    };
    let mut out = Vec::with_capacity(facts.modules.len() + facts.references.len());
    for module in &facts.modules {
        let kind = match module.kind {
            ModuleReferenceKind::Import => RelationKind::Imports,
            ModuleReferenceKind::ReExport => RelationKind::Exports,
        };
        let target = match &module.target {
            ModuleTarget::File(file) => RelationTarget::Resolved(Endpoint::File(file.clone())),
            ModuleTarget::External => {
                RelationTarget::Resolved(Endpoint::External(module.specifier.clone()))
            }
            ModuleTarget::Unresolved(reason) => RelationTarget::Unresolved {
                specifier: module.specifier.clone(),
                reason: *reason,
            },
        };
        out.push(Relationship {
            kind,
            from: Endpoint::File(path.clone()),
            target,
            location: at(module.range),
            type_only: module.type_only,
        });
    }
    for reference in &facts.references {
        let target = match &reference.target {
            ReferenceTarget::Declarations(sites) => {
                let Some(id) = sites
                    .iter()
                    .find_map(|site| symbols.declared_at(&site.path, site.name_start))
                else {
                    continue;
                };
                RelationTarget::Resolved(Endpoint::Symbol(id.clone()))
            }
            ReferenceTarget::Unresolved { specifier, reason } => RelationTarget::Unresolved {
                specifier: specifier.clone(),
                reason: *reason,
            },
        };
        let kind = match reference.kind {
            ReferenceKind::Call => RelationKind::Calls,
            ReferenceKind::Reference => RelationKind::References,
            ReferenceKind::Extends => RelationKind::Extends,
            ReferenceKind::Implements => RelationKind::Implements,
        };
        let from = symbols
            .enclosing(path, reference.range.start())
            .map_or_else(
                || Endpoint::File(path.clone()),
                |id| Endpoint::Symbol(id.clone()),
            );
        out.push(Relationship {
            kind,
            from,
            target,
            location: at(reference.range),
            type_only: false,
        });
    }
    // Stable: relationships at the same place keep the analyser's order.
    out.sort_by_key(|r| (r.location.range.start(), r.location.range.end(), r.kind));
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::symbol::{Declaration, DeclaredSymbol, SymbolKind, build_symbols};

    fn path(p: &str) -> RepoPath {
        RepoPath::new(p).unwrap()
    }

    fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range::new(Position::new(sl, sc), Position::new(el, ec)).unwrap()
    }

    /// `name` declared over lines `from..=to`, its name at `(from, 6)`.
    fn decl(name: &str, kind: SymbolKind, from: u32, to: u32) -> DeclaredSymbol {
        DeclaredSymbol {
            qualified_name: name.split('.').map(str::to_string).collect(),
            declaration: Declaration {
                range: range(from, 0, to, 1),
                name_range: range(from, 6, from, 6 + name.len() as u32),
                kind,
            },
            exported: false,
        }
    }

    /// `room.ts`: class `Room` (lines 0-9) with methods `open` (1-3) and `join` (5-8), then a
    /// function `start` (11-13) and a second declaration of interface `Room` (15-16).
    fn room() -> Vec<Symbol> {
        build_symbols(
            &path("room.ts"),
            vec![
                decl("Room", SymbolKind::Class, 0, 9),
                decl("Room.open", SymbolKind::Method, 1, 3),
                decl("Room.join", SymbolKind::Method, 5, 8),
                decl("start", SymbolKind::Function, 11, 13),
                decl("Room", SymbolKind::Interface, 15, 16),
            ],
            &BTreeSet::new(),
        )
    }

    fn table() -> SymbolTable {
        let other = build_symbols(
            &path("other.ts"),
            vec![decl("helper", SymbolKind::Function, 0, 2)],
            &BTreeSet::new(),
        );
        SymbolTable::new(room().iter().chain(other.iter()))
    }

    fn id(s: &str) -> SymbolId {
        SymbolId::new(&path("room.ts"), s)
    }

    #[test]
    fn the_innermost_declaration_encloses_a_position() {
        let table = table();
        let at =
            |line, character| table.enclosing(&path("room.ts"), Position::new(line, character));
        assert_eq!(at(2, 4), Some(&id("Room.open")));
        assert_eq!(at(6, 0), Some(&id("Room.join")));
        // Between the two methods, inside the class.
        assert_eq!(at(4, 2), Some(&id("Room")));
        // After the last method, still in the class; the end is exclusive.
        assert_eq!(at(9, 0), Some(&id("Room")));
        assert_eq!(at(9, 1), None);
        assert_eq!(at(10, 0), None);
        assert_eq!(at(12, 0), Some(&id("start")));
        // Every declaration of a merged symbol encloses.
        assert_eq!(at(15, 3), Some(&id("Room")));
        assert_eq!(
            table.enclosing(&path("missing.ts"), Position::new(0, 0)),
            None
        );
    }

    #[test]
    fn a_name_position_identifies_a_symbol() {
        let table = table();
        assert_eq!(
            table.declared_at(&path("room.ts"), Position::new(5, 6)),
            Some(&id("Room.join"))
        );
        // The second declaration of the merged `Room` names the same symbol.
        assert_eq!(
            table.declared_at(&path("room.ts"), Position::new(15, 6)),
            Some(&id("Room"))
        );
        assert_eq!(
            table.declared_at(&path("room.ts"), Position::new(5, 7)),
            None
        );
        assert_eq!(
            table.declared_at(&path("other.ts"), Position::new(0, 6)),
            Some(&SymbolId::new(&path("other.ts"), "helper"))
        );
    }

    fn site(p: &str, line: u32, character: u32) -> DeclarationSite {
        DeclarationSite {
            path: path(p),
            name_start: Position::new(line, character),
        }
    }

    fn call(line: u32, sites: Vec<DeclarationSite>) -> SymbolReference {
        SymbolReference {
            kind: ReferenceKind::Call,
            range: range(line, 2, line, 8),
            target: ReferenceTarget::Declarations(sites),
        }
    }

    #[test]
    fn linking_maps_places_to_ids() {
        let facts = SemanticFacts {
            modules: vec![
                ModuleReference {
                    kind: ModuleReferenceKind::Import,
                    specifier: "./other".into(),
                    range: range(0, 20, 0, 29),
                    type_only: false,
                    target: ModuleTarget::File(path("other.ts")),
                },
                ModuleReference {
                    kind: ModuleReferenceKind::ReExport,
                    specifier: "react".into(),
                    range: range(0, 40, 0, 47),
                    type_only: true,
                    target: ModuleTarget::External,
                },
                ModuleReference {
                    kind: ModuleReferenceKind::Import,
                    specifier: "name".into(),
                    range: range(12, 10, 12, 14),
                    type_only: false,
                    target: ModuleTarget::Unresolved(UnresolvedReason::Dynamic),
                },
            ],
            references: vec![
                // Inside `Room.open`, to `other.ts#helper`.
                call(2, vec![site("other.ts", 0, 6)]),
                // A local (no symbol there) is dropped.
                call(6, vec![site("room.ts", 6, 10)]),
                // The first declaration that is a symbol wins.
                call(7, vec![site("room.ts", 7, 3), site("room.ts", 1, 6)]),
                // Top-level code comes from the file; unresolved stays unresolved.
                SymbolReference {
                    kind: ReferenceKind::Reference,
                    range: range(10, 0, 10, 5),
                    target: ReferenceTarget::Unresolved {
                        specifier: "track".into(),
                        reason: UnresolvedReason::NotFound,
                    },
                },
                SymbolReference {
                    kind: ReferenceKind::Implements,
                    range: range(15, 20, 15, 25),
                    target: ReferenceTarget::Declarations(vec![site("other.ts", 0, 6)]),
                },
            ],
        };
        let summary: Vec<(String, String, String, String, bool)> =
            link(&path("room.ts"), &facts, &table())
                .iter()
                .map(|r| {
                    let target = match &r.target {
                        RelationTarget::Resolved(e) => e.id(),
                        RelationTarget::Unresolved { specifier, reason } => {
                            format!("? {specifier} ({reason})")
                        }
                    };
                    (
                        r.kind.to_string(),
                        r.from.id(),
                        target,
                        r.location.range.start().to_string(),
                        r.type_only,
                    )
                })
                .collect();
        let row = |k: &str, f: &str, t: &str, at: &str, type_only| {
            (k.into(), f.into(), t.into(), at.into(), type_only)
        };
        assert_eq!(
            summary,
            [
                row("imports", "file:room.ts", "file:other.ts", "1:21", false),
                row("exports", "file:room.ts", "external:react", "1:41", true),
                row(
                    "calls",
                    "symbol:room.ts#Room.open",
                    "symbol:other.ts#helper",
                    "3:3",
                    false
                ),
                row(
                    "calls",
                    "symbol:room.ts#Room.join",
                    "symbol:room.ts#Room.open",
                    "8:3",
                    false
                ),
                row(
                    "references",
                    "file:room.ts",
                    "? track (not_found)",
                    "11:1",
                    false
                ),
                row(
                    "imports",
                    "file:room.ts",
                    "? name (dynamic)",
                    "13:11",
                    false
                ),
                row(
                    "implements",
                    "symbol:room.ts#Room",
                    "symbol:other.ts#helper",
                    "16:21",
                    false
                ),
            ]
        );
    }

    #[test]
    fn linking_does_not_depend_on_the_order_of_facts() {
        let facts = SemanticFacts {
            modules: vec![],
            references: vec![
                call(12, vec![site("room.ts", 1, 6)]),
                call(2, vec![site("other.ts", 0, 6)]),
                call(7, vec![site("room.ts", 11, 6)]),
            ],
        };
        let mut reversed = facts.clone();
        reversed.references.reverse();
        let table = table();
        let a = link(&path("room.ts"), &facts, &table);
        assert_eq!(a, link(&path("room.ts"), &reversed, &table));
        let lines: Vec<u32> = a.iter().map(|r| r.location.range.start().line).collect();
        assert_eq!(lines, [2, 7, 12]);
    }

    #[test]
    fn nested_declarations_find_their_parents_in_one_sweep() {
        // A namespace (0-20) with a class (1-10) whose method (2-4) is followed by a sibling
        // method (6-8), then a function in the namespace (12-14).
        let symbols = build_symbols(
            &path("n.ts"),
            vec![
                decl("N", SymbolKind::Namespace, 0, 20),
                decl("N.C", SymbolKind::Class, 1, 10),
                decl("N.C.a", SymbolKind::Method, 2, 4),
                decl("N.C.b", SymbolKind::Method, 6, 8),
                decl("N.f", SymbolKind::Function, 12, 14),
            ],
            &BTreeSet::new(),
        );
        let table = SymbolTable::new(&symbols);
        let at = |line| {
            table
                .enclosing(&path("n.ts"), Position::new(line, 0))
                .map(|id| id.as_str().trim_start_matches("symbol:n.ts#").to_string())
        };
        let found: Vec<Option<String>> = [3, 5, 7, 9, 11, 13, 16, 21].map(at).to_vec();
        let want = ["N.C.a", "N.C", "N.C.b", "N.C", "N", "N.f", "N"].map(|s| Some(s.to_string()));
        assert_eq!(found[..7], want);
        assert_eq!(found[7], None);
    }

    #[test]
    fn reasons_and_kinds_use_the_contract_values() {
        assert_eq!(
            UnresolvedReason::ALL.map(UnresolvedReason::as_str),
            ["not_found", "ambiguous", "dynamic", "unsupported"]
        );
        let kinds = [
            RelationKind::Imports,
            RelationKind::Exports,
            RelationKind::Calls,
            RelationKind::References,
            RelationKind::Extends,
            RelationKind::Implements,
        ];
        assert_eq!(
            kinds.map(RelationKind::as_str),
            [
                "imports",
                "exports",
                "calls",
                "references",
                "extends",
                "implements"
            ]
        );
        assert_eq!(
            Endpoint::External("node:http".into()).id(),
            "external:node:http"
        );
        assert_eq!(Endpoint::File(path("a/b.ts")).id(), "file:a/b.ts");
    }
}
