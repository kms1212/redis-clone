use super::{CommandSpec, wrong_args_reply};
use crate::{db::Store, reply::Reply};

pub(crate) struct Get {
    key: Vec<u8>,
}

impl CommandSpec for Get {
    const NAME: &'static [u8] = b"GET";

    fn parse(args: Vec<Vec<u8>>) -> Result<Self, Reply> {
        let mut args = args.into_iter();
        match (args.next(), args.next()) {
            (Some(key), None) => Ok(Self { key }),
            _ => Err(wrong_args_reply(Self::NAME)),
        }
    }

    fn execute(self, store: &mut Store) -> Reply {
        match store.get(&self.key) {
            // A copy is unavoidable here: the store keeps its value, and the reply needs bytes
            // it owns, because the lock is released before the reply is written.
            Some(value) => Reply::Bulk(value.clone()),
            None => Reply::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{command::reply_in, db::Store, reply::Reply};

    #[test]
    fn get_returns_value_or_null() {
        let mut store = Store::new();
        assert_eq!(reply_in(&mut store, &[b"GET", b"k"]), Reply::Null);
        reply_in(&mut store, &[b"SET", b"k", b"v"]);
        assert_eq!(
            reply_in(&mut store, &[b"gEt", b"k"]),
            Reply::Bulk(b"v".to_vec())
        );
        // An empty value is a bulk string, not a null: difftest cannot tell them apart.
        reply_in(&mut store, &[b"SET", b"e", b""]);
        assert_eq!(
            reply_in(&mut store, &[b"GET", b"e"]),
            Reply::Bulk(Vec::new())
        );
    }

    #[test]
    fn get_needs_exactly_one_key() {
        let mut store = Store::new();
        let wrong = Reply::Error(b"ERR wrong number of arguments for 'get' command".to_vec());
        assert_eq!(reply_in(&mut store, &[b"GET"]), wrong);
        assert_eq!(reply_in(&mut store, &[b"GET", b"a", b"b"]), wrong);
    }
}
