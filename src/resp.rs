use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

use crate::error::{Error, ProtocolError};

/// Real Redis rejects a multibulk count above this.
const MAX_MULTIBULK_LENGTH: i64 = i32::MAX as i64;

/// Size limits applied while reading a request.
/// `Copy` because it is two numbers: every connection simply gets its own copy.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    /// Largest bulk string accepted, in bytes.
    pub(crate) message_bytes: usize,
    /// Longest `*<count>` or `$<length>` header line accepted, in bytes.
    pub(crate) header_bytes: usize,
}

/// Reads one header line and returns it without the terminator. `None` on a clean EOF.
/// Generic over the reader so tests can pass an in-memory buffer instead of a socket.
async fn read_header<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limits: Limits,
) -> Result<Option<Vec<u8>>, Error> {
    // Like real Redis, the line ends at `\r` and the byte after it is skipped without checking.
    // That is why `*1\n` is an invalid length rather than a complete header.
    let mut header = Vec::new();
    let bytes_read = reader
        .take(limits.header_bytes as u64)
        .read_until(b'\r', &mut header)
        .await?;
    if bytes_read == 0 {
        return Ok(None);
    }
    if header.pop() != Some(b'\r') {
        // Too long, or the client closed mid-line. Real Redis reports "too big ... string"
        // only past 64KB; we just close the connection.
        return Err(Error::InvalidRequest);
    }
    let mut line_feed = [0; 1];
    reader.read_exact(&mut line_feed).await?;
    Ok(Some(header))
}

/// Reads `*<count>`. `None` on a clean EOF.
pub(crate) async fn read_array_len<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limits: Limits,
) -> Result<Option<usize>, Error> {
    let Some(header) = read_header(reader, limits).await? else {
        return Ok(None);
    };
    let Some((b'*', digits)) = header.split_first() else {
        // Anything else would be an inline command, which we do not support.
        return Err(Error::InvalidRequest);
    };

    let count = parse_integer(digits)
        .filter(|count| *count <= MAX_MULTIBULK_LENGTH)
        .ok_or(ProtocolError::InvalidMultibulkLength)?;
    // Real Redis silently skips a zero or negative count, so it becomes an empty command.
    Ok(Some(usize::try_from(count).unwrap_or(0)))
}

/// Reads `$<length>` and the data after it.
pub(crate) async fn read_bulk<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let header = read_header(reader, limits)
        .await?
        .ok_or(Error::InvalidRequest)?;
    let digits = match header.split_first() {
        Some((b'$', digits)) => digits,
        Some((other, _)) => return Err(ProtocolError::ExpectedBulk(*other).into()),
        // An empty line: real Redis looks at the `\r` itself.
        None => return Err(ProtocolError::ExpectedBulk(b'\r').into()),
    };

    let length = parse_integer(digits)
        .and_then(|length| usize::try_from(length).ok())
        .filter(|length| *length <= limits.message_bytes)
        .ok_or(ProtocolError::InvalidBulkLength)?;

    let mut data = Vec::new();
    data.try_reserve_exact(length)
        .map_err(|_| Error::InvalidRequest)?;
    data.resize(length, 0);
    reader.read_exact(&mut data).await?;
    let mut terminator = [0; 2];
    reader.read_exact(&mut terminator).await?;
    if terminator != *b"\r\n" {
        return Err(Error::InvalidRequest);
    }
    Ok(data)
}

/// As strict as Redis's string2ll: no '+', no leading zeros, no "-0", no spaces.
fn parse_integer(bytes: &[u8]) -> Option<i64> {
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

    // `&[u8]` is itself an async buffered reader, so tests can parse straight from a byte string.
    const LIMITS: Limits = Limits {
        message_bytes: 1024,
        header_bytes: 64,
    };

    async fn array_len(mut input: &[u8]) -> Result<Option<usize>, Error> {
        read_array_len(&mut input, LIMITS).await
    }

    async fn bulk(mut input: &[u8]) -> Result<Vec<u8>, Error> {
        read_bulk(&mut input, LIMITS).await
    }

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

    #[tokio::test]
    async fn array_length() {
        assert!(matches!(array_len(b"*2\r\n").await, Ok(Some(2))));
        assert!(matches!(array_len(b"").await, Ok(None)));
        // Non-positive counts are skipped, not rejected.
        assert!(matches!(array_len(b"*-1\r\n").await, Ok(Some(0))));
        for input in [
            &b"*abc\r\n"[..],
            b"*\r\n",
            b"*1\n*1\r\n",
            b"*2147483648\r\n",
        ] {
            assert!(
                matches!(
                    array_len(input).await,
                    Err(Error::Protocol(ProtocolError::InvalidMultibulkLength))
                ),
                "{}",
                input.escape_ascii()
            );
        }
    }

    #[tokio::test]
    async fn bulk_string_errors() {
        assert_eq!(bulk(b"$2\r\nhi\r\n").await.unwrap(), b"hi");
        assert!(matches!(
            bulk(b"+PING\r\n").await,
            Err(Error::Protocol(ProtocolError::ExpectedBulk(b'+')))
        ));
        assert!(matches!(
            bulk(b"\r\n").await,
            Err(Error::Protocol(ProtocolError::ExpectedBulk(b'\r')))
        ));
        // Negative, malformed, and over the size limit (1024 here) are all the same error.
        for input in [&b"$-1\r\n"[..], b"$x\r\n", b"$1025\r\n"] {
            assert!(
                matches!(
                    bulk(input).await,
                    Err(Error::Protocol(ProtocolError::InvalidBulkLength))
                ),
                "{}",
                input.escape_ascii()
            );
        }
    }
}
