use super::{Command, wrong_args_reply};
use crate::{db::Store, reply::Reply};

/// `SET key value`. Options (EX, PX, NX, XX, ...) arrive in S04; until then any extra
/// argument is a syntax error, which matches real Redis only for unknown options.
pub(super) fn parse(args: Vec<Vec<u8>>) -> Result<Command, Reply> {
    let mut args = args.into_iter();
    match (args.next(), args.next(), args.next()) {
        (Some(key), Some(value), None) => Ok(Command::Set { key, value }),
        (Some(_), Some(_), Some(_)) => Err(Reply::Error(b"ERR syntax error".to_vec())),
        _ => Err(wrong_args_reply("set")),
    }
}

pub(super) fn execute(store: &mut Store, key: Vec<u8>, value: Vec<u8>) -> Reply {
    // Both are moved in: the bytes read from the socket become the stored value as is.
    store.insert(key, value);
    Reply::Simple("OK")
}

#[cfg(test)]
mod tests {
    use crate::{command::reply_in, db::Store, reply::Reply};

    #[test]
    fn set_overwrites_and_replies_ok() {
        let mut store = Store::new();
        assert_eq!(
            reply_in(&mut store, &[b"SET", b"k", b"v1"]),
            Reply::Simple("OK")
        );
        assert_eq!(
            reply_in(&mut store, &[b"SET", b"k", b"v2"]),
            Reply::Simple("OK")
        );
        assert_eq!(store.get(&b"k"[..]), Some(&b"v2".to_vec()));
    }

    #[test]
    fn set_argument_errors() {
        let mut store = Store::new();
        let wrong = Reply::Error(b"ERR wrong number of arguments for 'set' command".to_vec());
        assert_eq!(reply_in(&mut store, &[b"SET"]), wrong);
        assert_eq!(reply_in(&mut store, &[b"SET", b"k"]), wrong);
        assert_eq!(
            reply_in(&mut store, &[b"SET", b"k", b"v", b"foo"]),
            Reply::Error(b"ERR syntax error".to_vec())
        );
        assert!(store.is_empty());
    }
}
