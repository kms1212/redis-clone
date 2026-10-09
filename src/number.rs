//! Redis number syntax, shared by the protocol reader and by commands.

/// Parses a Redis integer, as strict as Redis's string2ll: no '+', no leading zeros, no "-0",
/// no spaces. Real Redis parses RESP lengths and numeric command arguments the same way.
///
/// Returns `None` rather than an error because the message depends on the caller
/// ("invalid bulk length", "value is not an integer or out of range", ...).
pub(crate) fn parse_integer(bytes: &[u8]) -> Option<i64> {
    let digits = bytes.strip_prefix(b"-").unwrap_or(bytes);
    let well_formed = match digits {
        [b'0'] => digits.len() == bytes.len(),
        [b'1'..=b'9', rest @ ..] => rest.iter().all(u8::is_ascii_digit),
        _ => false,
    };
    if !well_formed {
        return None;
    }
    // Overflow is rejected here too.
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_integer_is_as_strict_as_redis() {
        for (input, expected) in [(&b"0"[..], 0), (b"7", 7), (b"-12", -12)] {
            assert_eq!(parse_integer(input), Some(expected));
        }
        for input in [
            &b""[..],
            b"-",
            b"+1",
            b"05",
            b"-0",
            b" 1",
            b"1 ",
            b"99999999999999999999",
        ] {
            assert_eq!(parse_integer(input), None, "{}", input.escape_ascii());
        }
    }
}
