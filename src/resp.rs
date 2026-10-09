use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

use crate::error::{Error, ProtocolError};

/// Real Redis rejects a multibulk count above this.
const MAX_MULTIBULK_LENGTH: i64 = i32::MAX as i64;

/// Reads one header line and returns it without the terminator. `None` on a clean EOF.
/// Generic over the reader so tests can pass an in-memory buffer instead of a socket.
async fn read_header<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max_header_bytes: usize,
) -> Result<Option<Vec<u8>>, Error> {
    // Like real Redis, the line ends at `\r` and the byte after it is skipped without checking.
    // That is why `*1\n` is an invalid length rather than a complete header.
    let mut header = Vec::new();
    let bytes_read = reader
        .take(max_header_bytes as u64)
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
    max_header_bytes: usize,
) -> Result<Option<usize>, Error> {
    let Some(header) = read_header(reader, max_header_bytes).await? else {
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
    max_bytes: usize,
    max_header_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let header = read_header(reader, max_header_bytes)
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
        .filter(|length| *length <= max_bytes)
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

/// The rest of one command array, read from the stream only when a command asks for it.
pub(crate) struct Args<'a, R> {
    reader: &'a mut R,
    remaining: usize,
    max_bytes: usize,
    max_header_bytes: usize,
}

impl<'a, R: AsyncBufRead + Unpin> Args<'a, R> {
    pub(crate) fn new(
        reader: &'a mut R,
        count: usize,
        max_bytes: usize,
        max_header_bytes: usize,
    ) -> Self {
        Self {
            reader,
            remaining: count,
            max_bytes,
            max_header_bytes,
        }
    }

    /// Known from the array header, before any argument is read.
    pub(crate) fn remaining(&self) -> usize {
        self.remaining
    }

    pub(crate) async fn next(&mut self) -> Result<Option<Vec<u8>>, Error> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        read_bulk(self.reader, self.max_bytes, self.max_header_bytes)
            .await
            .map(Some)
    }

    /// Reads every remaining argument. Even when a command already knows its reply, the whole
    /// array must be consumed: real Redis reports a protocol error in a later element first,
    /// and the next command has to start at the right byte.
    pub(crate) async fn rest(&mut self) -> Result<Vec<Vec<u8>>, Error> {
        let mut rest = Vec::new();
        while let Some(arg) = self.next().await? {
            rest.push(arg);
        }
        Ok(rest)
    }
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

pub(crate) fn bulk_string(value: &[u8]) -> Vec<u8> {
    let mut reply = format!("${}\r\n", value.len()).into_bytes();
    reply.extend_from_slice(value);
    reply.extend_from_slice(b"\r\n");
    reply
}

pub(crate) fn error_reply(message: &[u8]) -> Vec<u8> {
    let mut reply = vec![b'-'];
    reply.extend_from_slice(message);
    reply.extend_from_slice(b"\r\n");
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    // `&[u8]` is itself an async buffered reader, so tests can parse straight from a byte string.
    async fn array_len(mut input: &[u8]) -> Result<Option<usize>, Error> {
        read_array_len(&mut input, 64).await
    }

    async fn bulk(mut input: &[u8]) -> Result<Vec<u8>, Error> {
        read_bulk(&mut input, 1024, 64).await
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
