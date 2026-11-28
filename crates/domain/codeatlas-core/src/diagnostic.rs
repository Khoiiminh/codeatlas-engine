//! Analysis diagnostics (ENG-02), mirroring the contract's `Diagnostic`.
//!
//! Codes are `CA` plus four digits and are stable: tools and users may filter on them. The
//! registry below is the single list of codes the engine emits. A code is never reused for a
//! different meaning.
//!
//! | Code | Severity | Meaning |
//! |---|---|---|
//! | `CA1001` | warning | The file parsed, but the parser had to recover from syntax errors. |
//! | `CA1002` | warning | The file could not be parsed at all (unreadable, changed since the scan, unsupported, too large, timed out, or the parser failed). |
//! | `CA1003` | warning | Not every symbol in the file was extracted, because it reached a limit (ENG-03). |
//! | `CA2001` | warning | Semantic analysis did not run for the file, or for the whole run, so its imports, calls and references are not resolved (ENG-04). |
//! | `CA2002` | warning | A TypeScript project configuration (`tsconfig.json`, `jsconfig.json`) has errors; the files it owns were analysed with what TypeScript could read of it (ENG-04). |

use std::fmt;

use crate::text::SourceLocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// The contract's wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// A diagnostic code: `CA` followed by exactly four ASCII digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagnosticCode([u8; 6]);

impl DiagnosticCode {
    /// The file parsed, but with syntax errors the parser recovered from.
    pub const PARSE_INCOMPLETE: DiagnosticCode = DiagnosticCode(*b"CA1001");
    /// The file could not be parsed at all.
    pub const PARSE_FAILED: DiagnosticCode = DiagnosticCode(*b"CA1002");
    /// Symbol extraction stopped at a limit, so some symbols are missing.
    pub const SYMBOLS_INCOMPLETE: DiagnosticCode = DiagnosticCode(*b"CA1003");
    /// Semantic analysis produced no result for a file, or did not run at all.
    pub const SEMANTICS_UNAVAILABLE: DiagnosticCode = DiagnosticCode(*b"CA2001");
    /// A project configuration file has errors.
    pub const PROJECT_CONFIG_PROBLEM: DiagnosticCode = DiagnosticCode(*b"CA2002");

    pub fn new(code: &str) -> Option<Self> {
        let bytes: [u8; 6] = code.as_bytes().try_into().ok()?;
        (bytes.starts_with(b"CA") && bytes[2..].iter().all(u8::is_ascii_digit))
            .then_some(DiagnosticCode(bytes))
    }

    pub fn as_str(&self) -> &str {
        // Always ASCII: checked by `new`, and the constants are ASCII literals.
        std::str::from_utf8(&self.0).unwrap_or("CA0000")
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub location: Option<SourceLocation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_ca_and_four_digits() {
        assert_eq!(
            DiagnosticCode::new("CA1001"),
            Some(DiagnosticCode::PARSE_INCOMPLETE)
        );
        assert_eq!(DiagnosticCode::PARSE_FAILED.to_string(), "CA1002");
        for bad in ["CA101", "CA10011", "ca1001", "XA1001", "CA10a1", ""] {
            assert_eq!(DiagnosticCode::new(bad), None, "{bad}");
        }
    }

    #[test]
    fn severities_use_the_contract_values() {
        let all = [Severity::Error, Severity::Warning, Severity::Info];
        assert_eq!(all.map(Severity::as_str), ["error", "warning", "info"]);
    }
}
