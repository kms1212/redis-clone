/// A reply to the client. Commands build one of these; only `encode` knows the wire format.
#[derive(Debug, PartialEq)]
pub(crate) enum Reply {
    /// `+PONG\r\n`. Always fixed text in Redis, so it borrows a `'static` string.
    Simple(&'static str),
    /// `-ERR ...\r\n`. Raw bytes: messages may quote client input that is not UTF-8.
    Error(Vec<u8>),
    /// `$5\r\nhello\r\n`. Binary safe: the length prefix, not a terminator, ends the data.
    Bulk(Vec<u8>),
}

impl Reply {
    pub(crate) fn encode(&self) -> Vec<u8> {
        match self {
            Self::Simple(text) => line(b'+', text.as_bytes()),
            Self::Error(message) => line(b'-', message),
            Self::Bulk(data) => {
                let mut out = format!("${}\r\n", data.len()).into_bytes();
                out.extend_from_slice(data);
                out.extend_from_slice(b"\r\n");
                out
            }
        }
    }
}

/// `<prefix><body>\r\n`, the one-line shape shared by simple strings and errors.
fn line(prefix: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 3);
    out.push(prefix);
    out.extend_from_slice(body);
    out.extend_from_slice(b"\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_each_type() {
        assert_eq!(Reply::Simple("PONG").encode(), b"+PONG\r\n");
        assert_eq!(Reply::Error(b"ERR x".to_vec()).encode(), b"-ERR x\r\n");
        assert_eq!(Reply::Bulk(b"hi".to_vec()).encode(), b"$2\r\nhi\r\n");
        assert_eq!(Reply::Bulk(Vec::new()).encode(), b"$0\r\n\r\n");
        // Length is counted in bytes: two Hangul syllables are 6 bytes.
        assert_eq!(
            Reply::Bulk("\u{d55c}\u{ae00}".into()).encode(),
            "$6\r\n\u{d55c}\u{ae00}\r\n".as_bytes()
        );
    }
}
