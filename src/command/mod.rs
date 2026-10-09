mod del;
mod echo;
mod exists;
mod get;
mod ping;
mod set;

use crate::{db::Store, reply::Reply};

/// A parsed command. Arguments are validated in `parse`, so every value here can be executed as is.
pub(crate) enum Command {
    Ping(Option<Vec<u8>>),
    Echo(Vec<u8>),
    Set { key: Vec<u8>, value: Vec<u8> },
    Get(Vec<u8>),
    Del(Vec<Vec<u8>>),
    Exists(Vec<Vec<u8>>),
}

impl Command {
    /// On failure, returns the error reply to send back to the client.
    pub(crate) fn parse(name: &[u8], args: Vec<Vec<u8>>) -> Result<Self, Reply> {
        if name.eq_ignore_ascii_case(b"PING") {
            return ping::parse(args);
        }
        if name.eq_ignore_ascii_case(b"ECHO") {
            return echo::parse(args);
        }
        if name.eq_ignore_ascii_case(b"SET") {
            return set::parse(args);
        }
        if name.eq_ignore_ascii_case(b"GET") {
            return get::parse(args);
        }
        if name.eq_ignore_ascii_case(b"DEL") {
            return del::parse(args);
        }
        if name.eq_ignore_ascii_case(b"EXISTS") {
            return exists::parse(args);
        }

        Err(unknown_command_reply(name, &args))
    }

    // Consumes self so the arguments it holds can be moved into the reply or the store
    // without copying.
    pub(crate) fn execute(self, store: &mut Store) -> Reply {
        match self {
            Self::Ping(message) => ping::execute(message),
            Self::Echo(message) => echo::execute(message),
            Self::Set { key, value } => set::execute(store, key, value),
            Self::Get(key) => get::execute(store, &key),
            Self::Del(keys) => del::execute(store, &keys),
            Self::Exists(keys) => exists::execute(store, &keys),
        }
    }
}

/// Real Redis uses the same message for every command, with the lowercase command name.
fn wrong_args_reply(name: &str) -> Reply {
    Reply::Error(format!("ERR wrong number of arguments for '{name}' command").into_bytes())
}

fn unknown_command_reply(name: &[u8], args: &[Vec<u8>]) -> Reply {
    // Built from raw bytes, not String: a non-UTF-8 name must be echoed back exactly as received.
    let mut message = b"ERR unknown command '".to_vec();
    message.extend_from_slice(name);
    message.push(b'\'');
    if !args.is_empty() {
        message.extend_from_slice(b", with args beginning with: ");
        for arg in args {
            // Real Redis appends a space after every argument, including the last one.
            message.push(b'\'');
            message.extend_from_slice(arg);
            message.extend_from_slice(b"' ");
        }
    }
    Reply::Error(message)
}

/// Test helper: runs one command against `store` and returns its reply.
#[cfg(test)]
pub(crate) fn reply_in(store: &mut Store, frame: &[&[u8]]) -> Reply {
    let args = frame[1..].iter().map(|arg| arg.to_vec()).collect();
    match Command::parse(frame[0], args) {
        Ok(command) => command.execute(store),
        Err(reply) => reply,
    }
}

/// Test helper for commands that do not touch the store.
#[cfg(test)]
pub(crate) fn reply_for(frame: &[&[u8]]) -> Reply {
    reply_in(&mut Store::new(), frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_message_depends_on_args() {
        assert_eq!(
            reply_for(&[b"garbage"]),
            Reply::Error(b"ERR unknown command 'garbage'".to_vec())
        );
        assert_eq!(
            reply_for(&[b"NOPE", b"a", b"b"]),
            Reply::Error(
                b"ERR unknown command 'NOPE', with args beginning with: 'a' 'b' ".to_vec()
            )
        );
    }
}
