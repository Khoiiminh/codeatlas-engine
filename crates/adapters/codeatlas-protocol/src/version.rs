//! Contract and protocol versions, and protocol version negotiation (ADR-0005).

/// Version of the contract bundle these types implement (`x-contract-version` in the schema).
pub const CONTRACT_VERSION: &str = "0.1.0";

/// JSON-RPC protocol version spoken by this engine (`x-protocol-version` in the schema).
pub const PROTOCOL_VERSION: &str = "0.1";

/// Parses a `MAJOR.MINOR` protocol version.
pub fn parse_protocol_version(version: &str) -> Option<(u64, u64)> {
    let (major, minor) = version.split_once('.')?;
    let is_number = |s: &str| {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'))
    };
    if !is_number(major) || !is_number(minor) {
        return None;
    }
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Two protocol versions are compatible when they share a compatibility key:
/// `MAJOR` for `MAJOR >= 1`, and `MAJOR.MINOR` while `MAJOR` is 0 (SemVer: in 0.x every
/// minor release may break).
pub fn is_protocol_compatible(a: &str, b: &str) -> bool {
    match (parse_protocol_version(a), parse_protocol_version(b)) {
        (Some((0, minor_a)), Some((0, minor_b))) => minor_a == minor_b,
        (Some((major_a, _)), Some((major_b, _))) => major_a != 0 && major_a == major_b,
        _ => false,
    }
}

/// Chooses the protocol version for a session: the first version in the client's list (most
/// preferred first) that is compatible with this engine. Returns `None` when there is none, in
/// which case the engine answers `initialize` with `UNSUPPORTED_PROTOCOL_VERSION`.
pub fn negotiate_protocol<'a>(
    client_versions: impl IntoIterator<Item = &'a str>,
) -> Option<&'static str> {
    client_versions
        .into_iter()
        .any(|v| is_protocol_compatible(v, PROTOCOL_VERSION))
        .then_some(PROTOCOL_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_major_minor() {
        assert_eq!(parse_protocol_version("0.1"), Some((0, 1)));
        assert_eq!(parse_protocol_version("12.0"), Some((12, 0)));
        for bad in ["", "1", "1.", ".1", "1.2.3", "01.2", "1.02", "a.b", "-1.0"] {
            assert_eq!(parse_protocol_version(bad), None, "{bad}");
        }
    }

    #[test]
    fn zero_major_needs_equal_minor() {
        assert!(is_protocol_compatible("0.1", "0.1"));
        assert!(!is_protocol_compatible("0.1", "0.2"));
    }

    #[test]
    fn stable_major_ignores_minor() {
        assert!(is_protocol_compatible("1.0", "1.4"));
        assert!(!is_protocol_compatible("1.0", "2.0"));
        assert!(!is_protocol_compatible("1.0", "0.1"));
    }

    #[test]
    fn negotiation_picks_engine_version_or_fails() {
        assert_eq!(negotiate_protocol(["2.0", "0.1"]), Some(PROTOCOL_VERSION));
        assert_eq!(negotiate_protocol(["0.2"]), None);
        assert_eq!(negotiate_protocol([]), None);
    }
}
