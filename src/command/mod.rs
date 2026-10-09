mod del;
mod echo;
mod exists;
mod get;
mod ping;
mod set;

use std::{collections::HashMap, sync::LazyLock};

use crate::{db::Store, reply::Reply};

/// Each command module's `parse`.
type Parser = fn(Vec<Vec<u8>>) -> Result<Command, Reply>;

/// Longest command name the table may hold. A longer name cannot be a known command.
const MAX_NAME_LEN: usize = 16;

/// Uppercase command name → its parser. Built once, the first time a command arrives.
/// Adding a command takes one line here, one `Command` variant, and one `execute` arm.
static COMMANDS: LazyLock<HashMap<&'static [u8], Parser>> = LazyLock::new(|| {
    let table: [(&'static [u8], Parser); 6] = [
        (b"PING", ping::parse),
        (b"ECHO", echo::parse),
        (b"SET", set::parse),
        (b"GET", get::parse),
        (b"DEL", del::parse),
        (b"EXISTS", exists::parse),
    ];
    HashMap::from(table)
});

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
        let mut buffer = [0; MAX_NAME_LEN];
        match uppercase(name, &mut buffer).and_then(|key| COMMANDS.get(key)) {
            Some(parse) => parse(args),
            None => Err(unknown_command_reply(name, &args)),
        }
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

/// Writes `name` in uppercase into `buffer` and returns that part, so looking a command up
/// allocates nothing. `None` if the name is too long to be any known command.
fn uppercase<'a>(name: &[u8], buffer: &'a mut [u8; MAX_NAME_LEN]) -> Option<&'a [u8]> {
    let upper = buffer.get_mut(..name.len())?;
    upper.copy_from_slice(name);
    upper.make_ascii_uppercase();
    Some(upper)
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

    #[test]
    fn table_names_are_uppercase_and_fit_the_buffer() {
        // A lowercase or too-long entry would compile fine but never match.
        for name in COMMANDS.keys() {
            assert!(name.len() <= MAX_NAME_LEN, "{}", name.escape_ascii());
            assert_eq!(*name, name.to_ascii_uppercase(), "{}", name.escape_ascii());
        }
    }

    #[test]
    fn names_longer_than_the_buffer_are_unknown() {
        let long = [b'A'; MAX_NAME_LEN + 1];
        assert_eq!(
            reply_for(&[&long]),
            Reply::Error(
                format!("ERR unknown command '{}'", "A".repeat(MAX_NAME_LEN + 1)).into_bytes()
            )
        );
    }
}
