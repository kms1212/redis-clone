use std::{fmt, io};

use crate::reply::Reply;

#[derive(Debug)]
pub(crate) enum Error {
    Io(io::Error),
    /// Malformed input we do not report to the client; the connection is just closed.
    InvalidRequest,
    /// Reported to the client as `-ERR Protocol error: ...`, then the connection is closed.
    Protocol(ProtocolError),
}

#[derive(Debug)]
pub(crate) enum ProtocolError {
    InvalidMultibulkLength,
    InvalidBulkLength,
    /// Holds the byte found where `$` was expected.
    ExpectedBulk(u8),
}

impl ProtocolError {
    /// The text after "Protocol error: ", shared by the client reply and the server log.
    /// Raw bytes rather than a String: `ExpectedBulk` may hold a non-UTF-8 byte that
    /// real Redis sends back unchanged.
    fn message(&self) -> Vec<u8> {
        match self {
            Self::InvalidMultibulkLength => b"invalid multibulk length".to_vec(),
            Self::InvalidBulkLength => b"invalid bulk length".to_vec(),
            Self::ExpectedBulk(got) => {
                // Real Redis replaces \r and \n with spaces so the error stays on one line.
                let got = match got {
                    b'\r' | b'\n' => b' ',
                    other => *other,
                };
                let mut message = b"expected '$', got '".to_vec();
                message.push(got);
                message.push(b'\'');
                message
            }
        }
    }

    pub(crate) fn reply(&self) -> Reply {
        let mut message = b"ERR Protocol error: ".to_vec();
        message.extend_from_slice(&self.message());
        Reply::Error(message)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ProtocolError> for Error {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidRequest => write!(formatter, "invalid request"),
            Self::Protocol(error) => write!(formatter, "protocol error: {error}"),
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message().escape_ascii())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_error_replies_match_real_redis() {
        assert_eq!(
            ProtocolError::InvalidMultibulkLength.reply(),
            Reply::Error(b"ERR Protocol error: invalid multibulk length".to_vec())
        );
        assert_eq!(
            ProtocolError::ExpectedBulk(b'+').reply(),
            Reply::Error(b"ERR Protocol error: expected '$', got '+'".to_vec())
        );
        // A line break would split the reply in two, so it becomes a space.
        assert_eq!(
            ProtocolError::ExpectedBulk(b'\r').reply(),
            Reply::Error(b"ERR Protocol error: expected '$', got ' '".to_vec())
        );
        // Non-UTF-8 bytes go out unchanged.
        assert_eq!(
            ProtocolError::ExpectedBulk(0xff).reply(),
            Reply::Error(b"ERR Protocol error: expected '$', got '\xff'".to_vec())
        );
    }
}
