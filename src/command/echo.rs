use super::{CommandSpec, wrong_args_reply};
use crate::{db::Store, reply::Reply};

/// ECHO takes exactly one argument.
pub(crate) struct Echo {
    message: Vec<u8>,
}

impl CommandSpec for Echo {
    const NAME: &'static [u8] = b"ECHO";

    fn parse(args: Vec<Vec<u8>>) -> Result<Self, Reply> {
        let mut args = args.into_iter();
        match (args.next(), args.next()) {
            (Some(message), None) => Ok(Self { message }),
            _ => Err(wrong_args_reply(Self::NAME)),
        }
    }

    fn execute(self, _store: &mut Store) -> Reply {
        Reply::Bulk(self.message)
    }
}

#[cfg(test)]
mod tests {
    use crate::{command::reply_for, reply::Reply};

    #[test]
    fn echo_replies_match_real_redis() {
        let hello = Reply::Bulk(b"hello".to_vec());
        assert_eq!(reply_for(&[b"ECHO", b"hello"]), hello);
        assert_eq!(reply_for(&[b"EcHo", b"hello"]), hello);
        // difftest cannot send an empty argument, so it is checked here.
        assert_eq!(reply_for(&[b"ECHO", b""]), Reply::Bulk(Vec::new()));
    }

    #[test]
    fn echo_needs_exactly_one_argument() {
        let wrong = Reply::Error(b"ERR wrong number of arguments for 'echo' command".to_vec());
        assert_eq!(reply_for(&[b"ECHO"]), wrong);
        assert_eq!(reply_for(&[b"ECHO", b"a", b"b"]), wrong);
    }
}
