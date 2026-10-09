use super::{Command, wrong_args_reply};
use crate::{db::Store, reply::Reply};

pub(super) fn parse(keys: Vec<Vec<u8>>) -> Result<Command, Reply> {
    if keys.is_empty() {
        return Err(wrong_args_reply("del"));
    }
    Ok(Command::Del(keys))
}

/// Replies with how many keys were actually removed. A key listed twice counts once,
/// because the second removal finds nothing.
pub(super) fn execute(store: &mut Store, keys: &[Vec<u8>]) -> Reply {
    let removed = keys
        .iter()
        .filter(|key| store.remove(*key).is_some())
        .count();
    // Fits: an array holds at most i32::MAX elements (see resp::read_array_len).
    Reply::Integer(removed as i64)
}

#[cfg(test)]
mod tests {
    use crate::{command::reply_in, db::Store, reply::Reply};

    #[test]
    fn del_counts_each_removed_key_once() {
        let mut store = Store::new();
        reply_in(&mut store, &[b"SET", b"a", b"1"]);
        reply_in(&mut store, &[b"SET", b"b", b"2"]);
        assert_eq!(
            reply_in(&mut store, &[b"DEL", b"a", b"b", b"nokey", b"a"]),
            Reply::Integer(2)
        );
        assert_eq!(reply_in(&mut store, &[b"DEL", b"a"]), Reply::Integer(0));
        assert!(store.is_empty());
    }

    #[test]
    fn del_needs_a_key() {
        assert_eq!(
            reply_in(&mut Store::new(), &[b"DEL"]),
            Reply::Error(b"ERR wrong number of arguments for 'del' command".to_vec())
        );
    }
}
