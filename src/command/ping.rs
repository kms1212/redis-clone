use super::{Command, wrong_args_reply};
use crate::reply::Reply;

/// `args` excludes the command name (`PING`).
pub(super) fn parse(args: Vec<Vec<u8>>) -> Result<Command, Reply> {
    // into_iter takes ownership of each argument instead of copying it.
    let mut args = args.into_iter();
    match (args.next(), args.next()) {
        (None, _) => Ok(Command::Ping(None)),
        (Some(message), None) => Ok(Command::Ping(Some(message))),
        _ => Err(wrong_args_reply("ping")),
    }
}

pub(super) fn execute(message: Option<Vec<u8>>) -> Reply {
    match message {
        None => Reply::Simple("PONG"),
        Some(message) => Reply::Bulk(message),
    }
}

#[cfg(test)]
mod tests {
    use crate::{command::reply_for, reply::Reply};

    #[test]
    fn ping_replies_match_real_redis() {
        assert_eq!(reply_for(&[b"PiNg"]), Reply::Simple("PONG"));
        assert_eq!(
            reply_for(&[b"PING", b"hello"]),
            Reply::Bulk(b"hello".to_vec())
        );
        assert_eq!(
            reply_for(&[b"PING", b"a", b"b"]),
            Reply::Error(b"ERR wrong number of arguments for 'ping' command".to_vec())
        );
    }
}
