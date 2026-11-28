//! Symbols declared in a source file, with stable identities (ENG-03).
//!
//! A parser reports each declaration it finds as a [`DeclaredSymbol`]. [`build_symbols`]
//! turns those into [`Symbol`]s. This module owns the identity rules, so every parser follows
//! the same ones:
//!
//! - **Identity is the file and the qualified name.** The id is `symbol:<path>#<qualifiedName>`,
//!   as in the contract examples and the FND-04 golden files. A member's qualified name is
//!   `Owner.member`; a namespace member's is `Namespace.member`. Nothing positional is part of
//!   the id, so editing other code in the file, or moving a declaration within it, keeps its id.
//!   Renaming the symbol or moving it to another file changes it.
//! - **One symbol per qualified name.** Declarations that share a qualified name in one file
//!   are one symbol with several declarations, as TypeScript merges them: function overloads,
//!   a getter and its setter, an interface declared twice, a class and an interface of the same
//!   name, a `const` and a `type` of the same name. Ids are therefore unique within a file. A
//!   static and an instance member of the same name also share one symbol.
//! - **Kind of a merged symbol.** The most specific kind among its declarations, in the order
//!   class, enum, function, method, variable, property, namespace, interface, type alias.
//!   Its location is the first declaration of that kind.
//! - **Exported** means exported from the module: declared with `export`, or named in a local
//!   export list (`export { a }`, `export default a`, `export = a`, CommonJS `module.exports`).
//!   Members are never module exports.
//! - **Long ids.** The contract limits ids to 512 characters. A longer id keeps its first 495
//!   characters and ends with `~` and 16 hexadecimal digits of a 64-bit FNV-1a hash of the
//!   full id. `~` cannot occur in a qualified name, so a shortened id never equals a full one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::source::RepoPath;
use crate::text::{Position, Range, SourceLocation};

/// What a symbol is. The wire values are the contract's `NodeKind` values for symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolKind {
    Namespace,
    Class,
    Interface,
    TypeAlias,
    Enum,
    Function,
    Method,
    Property,
    Variable,
}

impl SymbolKind {
    pub const ALL: [SymbolKind; 9] = [
        SymbolKind::Namespace,
        SymbolKind::Class,
        SymbolKind::Interface,
        SymbolKind::TypeAlias,
        SymbolKind::Enum,
        SymbolKind::Function,
        SymbolKind::Method,
        SymbolKind::Property,
        SymbolKind::Variable,
    ];

    /// The contract's wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Namespace => "namespace",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::TypeAlias => "type_alias",
            SymbolKind::Enum => "enum",
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Property => "property",
            SymbolKind::Variable => "variable",
        }
    }

    /// Which kind names a merged symbol: the lowest rank among its declarations.
    fn rank(self) -> u8 {
        match self {
            SymbolKind::Class => 0,
            SymbolKind::Enum => 1,
            SymbolKind::Function => 2,
            SymbolKind::Method => 3,
            SymbolKind::Variable => 4,
            SymbolKind::Property => 5,
            SymbolKind::Namespace => 6,
            SymbolKind::Interface => 7,
            SymbolKind::TypeAlias => 8,
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable symbol identifier: `symbol:<path>#<qualifiedName>`, at most 512 characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(String);

impl SymbolId {
    /// The contract's `NodeId` limit, in characters.
    pub const MAX_CHARS: usize = 512;

    pub fn new(path: &RepoPath, qualified_name: &str) -> SymbolId {
        let full = format!("symbol:{path}#{qualified_name}");
        if full.chars().count() <= Self::MAX_CHARS {
            return SymbolId(full);
        }
        let suffix = format!("~{:016x}", fnv1a_64(full.as_bytes()));
        let mut id: String = full.chars().take(Self::MAX_CHARS - suffix.len()).collect();
        id.push_str(&suffix);
        SymbolId(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 64-bit FNV-1a: small, fixed, and the same on every platform and release.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// One place where a symbol is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Declaration {
    /// The whole declaration, including decorators and modifiers such as `export`, and a
    /// member's trailing `;` or `,`. For a variable, the declarator only (`a = 1`, not
    /// `const a = 1;`); for a destructured variable, its element of the pattern.
    pub range: Range,
    /// The name. For an anonymous default export (`export default function () {}`), the
    /// `default` keyword.
    pub name_range: Range,
    pub kind: SymbolKind,
}

/// A declaration as a parser found it, before declarations are merged into symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredSymbol {
    /// The name, after the names of its owners: `["PartyRoom", "open"]`. Every owner must be
    /// declared too.
    pub qualified_name: Vec<String>,
    pub declaration: Declaration,
    /// Declared with an `export` modifier at module scope.
    pub exported: bool,
}

/// A named declaration at module scope, or a member of a class, interface or namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    id: SymbolId,
    path: RepoPath,
    name: String,
    qualified_name: String,
    kind: SymbolKind,
    parent: Option<SymbolId>,
    exported: bool,
    /// In document order; never empty.
    declarations: Vec<Declaration>,
}

impl Symbol {
    pub fn id(&self) -> &SymbolId {
        &self.id
    }

    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// The last segment of the qualified name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `Owner.member` for a member, the name itself at module scope.
    pub fn qualified_name(&self) -> &str {
        &self.qualified_name
    }

    pub fn kind(&self) -> SymbolKind {
        self.kind
    }

    /// The owning class, interface or namespace.
    pub fn parent(&self) -> Option<&SymbolId> {
        self.parent.as_ref()
    }

    pub fn exported(&self) -> bool {
        self.exported
    }

    /// Every declaration, in document order.
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    /// The declaration that locates the symbol: the first one of the symbol's kind.
    pub fn primary(&self) -> &Declaration {
        self.declarations
            .iter()
            .find(|d| d.kind == self.kind)
            .unwrap_or(&self.declarations[0])
    }

    /// The primary declaration's range, for the contract's `Node.location`.
    pub fn location(&self) -> SourceLocation {
        SourceLocation {
            path: self.path.clone(),
            range: self.primary().range,
        }
    }
}

/// Merges the declarations of the file at `path` into symbols, sorted by the start of their
/// first declaration, then by qualified name. The order of `declared` does not matter.
///
/// `exported_names` are module-scope names that a local export list exports. Names that
/// match no declared symbol (re-exported imports, for example) are ignored.
pub fn build_symbols(
    path: &RepoPath,
    declared: Vec<DeclaredSymbol>,
    exported_names: &BTreeSet<String>,
) -> Vec<Symbol> {
    struct Group {
        segments: Vec<String>,
        declarations: Vec<Declaration>,
        exported: bool,
    }
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for d in declared {
        if d.qualified_name.is_empty() {
            continue;
        }
        let group = groups
            .entry(d.qualified_name.join("."))
            .or_insert_with(|| Group {
                segments: d.qualified_name,
                declarations: Vec::new(),
                exported: false,
            });
        group.declarations.push(d.declaration);
        group.exported |= d.exported;
    }

    let mut symbols: Vec<Symbol> = groups
        .into_iter()
        .map(|(qualified_name, mut group)| {
            group.declarations.sort();
            group.declarations.dedup();
            let kind = group
                .declarations
                .iter()
                .map(|d| d.kind)
                .min_by_key(|k| k.rank())
                .unwrap_or(SymbolKind::Variable);
            let module_scope = group.segments.len() == 1;
            let name = group.segments.last().cloned().unwrap_or_default();
            let exported =
                module_scope && (group.exported || exported_names.contains(&qualified_name));
            let parent = (!module_scope).then(|| {
                SymbolId::new(path, &group.segments[..group.segments.len() - 1].join("."))
            });
            Symbol {
                id: SymbolId::new(path, &qualified_name),
                path: path.clone(),
                name,
                qualified_name,
                kind,
                parent,
                exported,
                declarations: group.declarations,
            }
        })
        .collect();
    symbols.sort_by(|a, b| {
        (a.declarations[0].range.start(), &a.qualified_name)
            .cmp(&(b.declarations[0].range.start(), &b.qualified_name))
    });
    symbols
}

/// Why a parser stopped extracting symbols before the end of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolLimit {
    /// The file declares more than `limit` symbols.
    Count { limit: usize },
    /// Namespaces are nested more than `limit` levels deep.
    NamespaceDepth { limit: usize },
}

/// Symbol extraction stopped at `at`, the first declaration that was not extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolsTruncated {
    pub at: Position,
    pub limit: SymbolLimit,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> RepoPath {
        RepoPath::new("src/room.ts").unwrap()
    }

    fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range::new(Position::new(sl, sc), Position::new(el, ec)).unwrap()
    }

    fn decl(name: &str, kind: SymbolKind, line: u32, exported: bool) -> DeclaredSymbol {
        DeclaredSymbol {
            qualified_name: name.split('.').map(str::to_string).collect(),
            declaration: Declaration {
                range: range(line, 0, line + 1, 1),
                name_range: range(line, 6, line, 6 + name.len() as u32),
                kind,
            },
            exported,
        }
    }

    #[test]
    fn ids_follow_the_contract_examples() {
        let id = SymbolId::new(&RepoPath::new("src/app.ts").unwrap(), "start");
        assert_eq!(id.as_str(), "symbol:src/app.ts#start");
        assert_eq!(
            SymbolId::new(&path(), "PartyRoom.open").to_string(),
            "symbol:src/room.ts#PartyRoom.open"
        );
    }

    #[test]
    fn long_ids_are_shortened_with_a_hash_and_stay_distinct() {
        let long = "a".repeat(600);
        let a = SymbolId::new(&path(), &format!("{long}.x"));
        let b = SymbolId::new(&path(), &format!("{long}.y"));
        assert_eq!(a.as_str().chars().count(), SymbolId::MAX_CHARS);
        assert_ne!(a, b);
        assert!(a.as_str().starts_with("symbol:src/room.ts#aaa"));
        let suffix = &a.as_str()[a.as_str().len() - 17..];
        assert!(suffix.starts_with('~'));
        assert!(suffix[1..].chars().all(|c| c.is_ascii_hexdigit()));
        // Deterministic: the same input gives the same id.
        assert_eq!(a, SymbolId::new(&path(), &format!("{long}.x")));
        // Characters, not bytes: 500 two-byte characters still fit.
        let wide = SymbolId::new(&path(), &"é".repeat(490));
        assert!(!wide.as_str().contains('~'));
        // The hash is FNV-1a 64 (test vector: "a" -> af63dc4c8601ec8c).
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn members_get_their_owner_as_parent_and_are_never_exported() {
        let symbols = build_symbols(
            &path(),
            vec![
                decl("PartyRoom", SymbolKind::Class, 0, true),
                decl("PartyRoom.open", SymbolKind::Method, 3, true),
            ],
            &BTreeSet::new(),
        );
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].qualified_name(), "PartyRoom");
        assert!(symbols[0].exported());
        assert_eq!(symbols[0].parent(), None);
        assert_eq!(symbols[1].name(), "open");
        assert_eq!(symbols[1].parent(), Some(symbols[0].id()));
        assert!(!symbols[1].exported());
    }

    #[test]
    fn declarations_with_one_name_merge_into_one_symbol() {
        // An interface and a class `Room` (declaration merging), an overloaded function,
        // and a `const` and a `type` with one name.
        let symbols = build_symbols(
            &path(),
            vec![
                decl("Room", SymbolKind::Interface, 0, false),
                decl("Room", SymbolKind::Class, 4, true),
                decl("open", SymbolKind::Function, 8, false),
                decl("open", SymbolKind::Function, 9, false),
                decl("Status", SymbolKind::TypeAlias, 12, false),
                decl("Status", SymbolKind::Variable, 11, false),
            ],
            &BTreeSet::new(),
        );
        let summary: Vec<(&str, SymbolKind, usize, u32)> = symbols
            .iter()
            .map(|s| {
                (
                    s.qualified_name(),
                    s.kind(),
                    s.declarations().len(),
                    s.primary().range.start().line,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("Room", SymbolKind::Class, 2, 4),
                ("open", SymbolKind::Function, 2, 8),
                ("Status", SymbolKind::Variable, 2, 11),
            ]
        );
        // One exported declaration makes the symbol exported.
        assert!(symbols[0].exported());
        assert_eq!(symbols[0].location().range.start().line, 4);
    }

    #[test]
    fn export_lists_export_module_scope_names_only() {
        let exported: BTreeSet<String> = ["a", "Box.size", "imported"]
            .into_iter()
            .map(String::from)
            .collect();
        let symbols = build_symbols(
            &path(),
            vec![
                decl("a", SymbolKind::Variable, 0, false),
                decl("Box", SymbolKind::Class, 1, false),
                decl("Box.size", SymbolKind::Property, 2, false),
            ],
            &exported,
        );
        let flags: Vec<(&str, bool)> = symbols
            .iter()
            .map(|s| (s.qualified_name(), s.exported()))
            .collect();
        assert_eq!(flags, [("a", true), ("Box", false), ("Box.size", false)]);
    }

    #[test]
    fn the_result_does_not_depend_on_the_order_of_declarations() {
        let declared = vec![
            decl("A", SymbolKind::Class, 0, true),
            decl("A.m", SymbolKind::Method, 1, false),
            decl("A.m", SymbolKind::Method, 2, false),
            decl("f", SymbolKind::Function, 5, false),
            decl("A", SymbolKind::Interface, 7, false),
        ];
        let expected = build_symbols(&path(), declared.clone(), &BTreeSet::new());
        let mut reversed = declared.clone();
        reversed.reverse();
        assert_eq!(build_symbols(&path(), reversed, &BTreeSet::new()), expected);
        let mut rotated = declared;
        rotated.rotate_left(2);
        assert_eq!(build_symbols(&path(), rotated, &BTreeSet::new()), expected);
    }

    #[test]
    fn kinds_use_the_contract_values() {
        let values: Vec<&str> = SymbolKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            values,
            [
                "namespace",
                "class",
                "interface",
                "type_alias",
                "enum",
                "function",
                "method",
                "property",
                "variable"
            ]
        );
    }
}
