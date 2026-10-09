use super::{Command, wrong_args_reply};
use crate::{db::Store, reply::Reply};

pub(super) fn parse(keys: Vec<Vec<u8>>) -> Result<Command, Reply> {
    if keys.is_empty() {
        return Err(wrong_args_reply("exists"));
    }
    Ok(Command::Exists(keys))
}

/// Replies with how many of the listed keys exist. Unlike DEL, a key listed twice counts twice.
pub(super) fn execute(store: &Store, keys: &[Vec<u8>]) -> Reply {
    let found = keys.iter().filter(|key| store.contains_key(*key)).count();
    // Fits: an array holds at most i32::MAX elements (see resp::read_array_len).
    Reply::Integer(found as i64)
}

#[cfg(test)]
mod tests {
    use crate::{command::reply_in, db::Store, reply::Reply};

    #[test]
    fn exists_counts_duplicates_separately() {
        let mut store = Store::new();
        reply_in(&mut store, &[b"SET", b"c", b"3"]);
        assert_eq!(reply_in(&mut store, &[b"EXISTS", b"c"]), Reply::Integer(1));
        assert_eq!(
            reply_in(&mut store, &[b"EXISTS", b"c", b"c", b"nokey"]),
            Reply::Integer(2)
        );
        assert_eq!(
            reply_in(&mut store, &[b"EXISTS", b"nokey"]),
            Reply::Integer(0)
        );
    }

    #[test]
    fn exists_needs_a_key() {
        assert_eq!(
            reply_in(&mut Store::new(), &[b"EXISTS"]),
            Reply::Error(b"ERR wrong number of arguments for 'exists' command".to_vec())
        );
    }
}
