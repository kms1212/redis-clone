use super::Command;
use crate::resp::{bulk_string, error_reply};

/// `args` excludes the command name (`PING`).
pub(super) fn parse(args: Vec<Vec<u8>>) -> Result<Command, Vec<u8>> {
    // into_iter takes ownership of each argument instead of copying it.
    let mut args = args.into_iter();
    match (args.next(), args.next()) {
        (None, _) => Ok(Command::Ping(None)),
        (Some(message), None) => Ok(Command::Ping(Some(message))),
        _ => Err(error_reply(
            b"ERR wrong number of arguments for 'ping' command",
        )),
    }
}

pub(super) fn execute(message: Option<Vec<u8>>) -> Vec<u8> {
    match message {
        None => b"+PONG\r\n".to_vec(),
        Some(message) => bulk_string(&message),
    }
}

#[cfg(test)]
mod tests {
    use crate::command::reply_for;

    #[test]
    fn ping_replies_match_real_redis() {
        assert_eq!(reply_for(&[b"PiNg"]), b"+PONG\r\n");
        assert_eq!(
            reply_for(&[b"PING", "\u{d55c}\u{ae00}".as_bytes()]),
            "$6\r\n\u{d55c}\u{ae00}\r\n".as_bytes()
        );
        assert_eq!(
            reply_for(&[b"PING", b"a", b"b"]),
            b"-ERR wrong number of arguments for 'ping' command\r\n"
        );
    }
}
