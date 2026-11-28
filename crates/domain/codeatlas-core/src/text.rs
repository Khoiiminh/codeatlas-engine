//! Positions in source text (ENG-02), in the contract's units.
//!
//! The contract (`Position`, `Range`, `SourceLocation`) uses zero-based lines and counts
//! `character` in UTF-16 code units, as VS Code and the Language Server Protocol do. Parsers
//! work in UTF-8 byte offsets, so [`LineIndex`] converts byte offsets into positions.
//!
//! Line breaks are `\n`, `\r\n` and a lone `\r`, as in the Language Server Protocol. The
//! Unicode separators U+2028 and U+2029 are not line breaks here, also as in LSP and VS Code,
//! even though JavaScript treats them as line terminators.

use std::fmt;

use crate::source::RepoPath;

/// A zero-based position: `line`, and `character` in UTF-16 code units within that line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Position { line, character }
    }
}

impl fmt::Display for Position {
    /// One-based `line:column`, the way editors and compilers print positions.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}",
            u64::from(self.line) + 1,
            u64::from(self.character) + 1
        )
    }
}

/// A half-open range `[start, end)`. Invariant: `start <= end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Range {
    start: Position,
    end: Position,
}

/// A [`Range`] whose end comes before its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReversedRange {
    pub start: Position,
    pub end: Position,
}

impl fmt::Display for ReversedRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "range end {} is before its start {}",
            self.end, self.start
        )
    }
}

impl std::error::Error for ReversedRange {}

impl Range {
    pub fn new(start: Position, end: Position) -> Result<Self, ReversedRange> {
        if end < start {
            return Err(ReversedRange { start, end });
        }
        Ok(Range { start, end })
    }

    /// An empty range at `at`, for example where a parser expected a missing token.
    pub const fn empty(at: Position) -> Self {
        Range { start: at, end: at }
    }

    pub const fn start(&self) -> Position {
        self.start
    }

    pub const fn end(&self) -> Position {
        self.end
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// A range in one repository file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceLocation {
    pub path: RepoPath,
    pub range: Range,
}

/// Converts UTF-8 byte offsets in one text into [`Position`]s.
///
/// Building the index scans the text once. Each lookup is a binary search for the line,
/// then a count of UTF-16 units from the start of that line.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    /// Byte offset where each line starts. Always starts with 0.
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let mut line_starts = vec![0];
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => line_starts.push(i + 1),
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                    line_starts.push(i + 2);
                    i += 1;
                }
                b'\r' => line_starts.push(i + 1),
                _ => {}
            }
            i += 1;
        }
        LineIndex { text, line_starts }
    }

    /// Number of lines. An empty text, and a text ending in a line break, end with an empty line.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The position of byte `offset`.
    ///
    /// An offset past the end gives the end of the text. An offset inside a multi-byte
    /// character, or between the `\r` and `\n` of a `\r\n`, moves back to the nearest valid
    /// place, so every result names a real place in the text.
    pub fn position(&self, offset: usize) -> Position {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line];
        let mut line_text = &self.text[line_start..offset];
        // The `\n` of a `\r\n` belongs to the previous line's break: stop before the `\r`.
        if let Some(stripped) = line_text.strip_suffix('\r') {
            line_text = stripped;
        }
        let character: usize = line_text.chars().map(char::len_utf16).sum();
        Position::new(saturate(line), saturate(character))
    }

    /// The range covering bytes `start..end`. The ends are swapped if `end < start`.
    pub fn range(&self, start: usize, end: usize) -> Range {
        let (a, b) = (self.position(start.min(end)), self.position(start.max(end)));
        Range::new(a, b).unwrap_or(Range::empty(a))
    }
}

fn saturate(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    #[test]
    fn counts_lines_for_every_line_break_style() {
        let index = LineIndex::new("a\nb\r\nc\rd");
        assert_eq!(index.line_count(), 4);
        assert_eq!(index.position(0), pos(0, 0));
        assert_eq!(index.position(2), pos(1, 0)); // b
        assert_eq!(index.position(5), pos(2, 0)); // c
        assert_eq!(index.position(7), pos(3, 0)); // d
        assert_eq!(index.position(8), pos(3, 1)); // end of text
        assert_eq!(LineIndex::new("").line_count(), 1);
        assert_eq!(LineIndex::new("x\n").line_count(), 2);
    }

    #[test]
    fn crlf_offsets_stay_on_their_line() {
        let index = LineIndex::new("ab\r\ncd");
        assert_eq!(index.position(2), pos(0, 2)); // the \r
        assert_eq!(index.position(3), pos(0, 2)); // the \n of \r\n
        assert_eq!(index.position(4), pos(1, 0));
    }

    #[test]
    fn characters_are_utf16_code_units() {
        // é is 2 bytes and 1 unit; 世 is 3 bytes and 1 unit; 😀 is 4 bytes and 2 units.
        let text = "é世😀x";
        let index = LineIndex::new(text);
        assert_eq!(index.position(text.find('x').unwrap()), pos(0, 4));
        assert_eq!(index.position(3), pos(0, 1)); // inside 世: back to its start
        assert_eq!(index.position(100), pos(0, 5));
    }

    #[test]
    fn unicode_separators_are_not_line_breaks() {
        let text = "a\u{2028}b";
        assert_eq!(LineIndex::new(text).position(text.len()), pos(0, 3));
    }

    #[test]
    fn ranges_are_ordered_and_display_one_based() {
        let index = LineIndex::new("one\ntwo");
        let range = index.range(6, 4);
        assert_eq!((range.start(), range.end()), (pos(1, 0), pos(1, 2)));
        assert!(Range::new(pos(1, 0), pos(0, 5)).is_err());
        assert!(Range::empty(pos(2, 3)).is_empty());
        assert_eq!(pos(0, 0).to_string(), "1:1");
    }
}
