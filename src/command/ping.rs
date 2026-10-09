use super::{CommandSpec, wrong_args_reply};
use crate::{db::Store, reply::Reply};

pub(crate) struct Ping {
    message: Option<Vec<u8>>,
}

impl CommandSpec for Ping {
    const NAME: &'static [u8] = b"PING";

    fn parse(args: Vec<Vec<u8>>) -> Result<Self, Reply> {
        // into_iter takes ownership of each argument instead of copying it.
        let mut args = args.into_iter();
        match (args.next(), args.next()) {
            (message, None) => Ok(Self { message }),
            _ => Err(wrong_args_reply(Self::NAME)),
        }
    }

    fn execute(self, _store: &mut Store) -> Reply {
        match self.message {
            None => Reply::Simple("PONG"),
            Some(message) => Reply::Bulk(message),
        }
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
