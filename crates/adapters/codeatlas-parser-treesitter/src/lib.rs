//! codeatlas-parser-treesitter: Adapter (driven): Tree-sitter LanguageParser (ENG-02).
//!
//! Parses TypeScript, TSX, JavaScript and JSX with the official Tree-sitter grammars and
//! reports where the parser recovered from syntax errors. Tree-sitter types never leave this
//! crate: the result is a `codeatlas_core::ParsedSource`.
//!
//! Grammar per language:
//!
//! | Language | Grammar |
//! |---|---|
//! | TypeScript (`.ts`, `.mts`, `.cts`) | `typescript`: no JSX, as in `tsc` |
//! | TSX (`.tsx`) | `tsx` |
//! | JavaScript (`.js`, `.mjs`, `.cjs`), JSX (`.jsx`) | `javascript`, which accepts JSX in `.js` files as bundlers do |
//!
//! Allowed workspace dependencies are listed in crates/README.md (ADR-0010).

use std::ops::ControlFlow;
use std::time::Instant;

use codeatlas_application::{LanguageParser, ParseError, ParseInput, ParseLimits};
use codeatlas_core::{Language, LineIndex, ParsedSource, SyntaxProblem, SyntaxProblemKind};
use tree_sitter::{Node, ParseOptions, Parser, Tree};

const BOM: &str = "\u{feff}";

/// Tree-sitter addresses text with 32-bit byte offsets.
const MAX_TEXT_BYTES: u64 = u32::MAX as u64;

/// [`LanguageParser`] backed by Tree-sitter. It holds no state, so one value can serve any
/// number of threads; each call creates its own Tree-sitter parser.
#[derive(Debug, Default, Clone, Copy)]
pub struct TreeSitterParser;

impl TreeSitterParser {
    /// The Tree-sitter grammar used for `language`.
    pub fn grammar(language: Language) -> tree_sitter::Language {
        match language {
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::JavaScript | Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),
        }
    }

    /// Parses `text` (without a leading byte order mark) into a Tree-sitter tree.
    fn tree(text: &str, language: Language, limits: &ParseLimits) -> Result<Tree, ParseError> {
        let bytes = text.len() as u64;
        if bytes > MAX_TEXT_BYTES {
            return Err(ParseError::TooLarge {
                bytes,
                limit_bytes: MAX_TEXT_BYTES,
            });
        }
        let mut parser = Parser::new();
        parser
            .set_language(&Self::grammar(language))
            .map_err(|e| ParseError::Failed(e.to_string()))?;

        let source = text.as_bytes();
        let mut read = |offset: usize, _| source.get(offset..).unwrap_or_default();
        let started = Instant::now();
        let mut timed_out = false;
        let mut progress = |_: &tree_sitter::ParseState| match limits.timeout {
            Some(limit) if started.elapsed() >= limit => {
                timed_out = true;
                ControlFlow::Break(())
            }
            _ => ControlFlow::Continue(()),
        };
        let options = ParseOptions::new().progress_callback(&mut progress);
        let tree = parser.parse_with_options(&mut read, None, Some(options));
        match tree {
            Some(tree) => Ok(tree),
            None if timed_out => Err(ParseError::TimedOut {
                after: limits.timeout.unwrap_or_default(),
            }),
            None => Err(ParseError::Failed(
                "Tree-sitter returned no tree".to_string(),
            )),
        }
    }
}

impl LanguageParser for TreeSitterParser {
    fn parse(
        &self,
        input: &ParseInput<'_>,
        limits: &ParseLimits,
    ) -> Result<ParsedSource, ParseError> {
        // Editors drop a byte order mark from the document text, so positions do too.
        let text = input.text.strip_prefix(BOM).unwrap_or(input.text);
        let tree = Self::tree(text, input.language, limits)?;
        let (problems, truncated) = syntax_problems(tree.root_node(), text, limits.max_problems);
        Ok(ParsedSource::new(
            input.path.clone(),
            input.language,
            problems,
            truncated,
        ))
    }
}

/// Every recovery point under `root`, in document order, up to `max`. Returns whether there
/// were more.
///
/// An `ERROR` node covers text the parser skipped; it is reported once, without looking
/// inside it. A missing node is a zero-width token the parser assumed. Only subtrees that
/// contain an error are visited, so a clean file costs one check of the root.
fn syntax_problems(root: Node<'_>, text: &str, max: usize) -> (Vec<SyntaxProblem>, bool) {
    let mut problems = Vec::new();
    if !root.has_error() {
        return (problems, false);
    }
    let index = LineIndex::new(text);
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let problem = if node.is_error() {
            Some(SyntaxProblemKind::Unexpected)
        } else if node.is_missing() {
            Some(SyntaxProblemKind::Missing {
                expected: node.kind().to_string(),
            })
        } else {
            None
        };
        let found = problem.is_some();
        if let Some(kind) = problem {
            if problems.len() == max {
                return (problems, true);
            }
            problems.push(SyntaxProblem {
                range: index.range(node.start_byte(), node.end_byte()),
                kind,
            });
        }
        // Descend only into clean-looking nodes that contain an error somewhere below.
        let descend = !found && node.has_error() && cursor.goto_first_child();
        if descend {
            continue;
        }
        // Move to the next sibling, climbing up as needed; stop when back at the root.
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return (problems, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_compatible_grammar() {
        for language in [
            Language::TypeScript,
            Language::Tsx,
            Language::JavaScript,
            Language::Jsx,
        ] {
            Parser::new()
                .set_language(&TreeSitterParser::grammar(language))
                .unwrap_or_else(|e| panic!("{}: {e}", language.as_str()));
        }
    }
}
