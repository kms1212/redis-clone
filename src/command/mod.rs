mod echo;
mod ping;

use crate::resp::error_reply;

/// A parsed command. Arguments are validated in `parse`, so every value here can be executed as is.
pub(crate) enum Command {
    Ping(Option<Vec<u8>>),
    Echo(Vec<u8>),
}

impl Command {
    /// On failure, returns the error reply to send back to the client unchanged.
    pub(crate) fn parse(name: &[u8], args: Vec<Vec<u8>>) -> Result<Self, Vec<u8>> {
        if name.eq_ignore_ascii_case(b"PING") {
            return ping::parse(args);
        }
        if name.eq_ignore_ascii_case(b"ECHO") {
            return echo::parse(args);
        }

        Err(unknown_command_reply(name, &args))
    }

    // Consumes self so the arguments it holds can be moved into the reply without copying.
    pub(crate) fn execute(self) -> Vec<u8> {
        match self {
            Self::Ping(message) => ping::execute(message),
            Self::Echo(message) => echo::execute(message),
        }
    }
}

fn unknown_command_reply(name: &[u8], args: &[Vec<u8>]) -> Vec<u8> {
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
    error_reply(&message)
}

/// Test helper: "what reply do these arguments produce?" in one line.
#[cfg(test)]
pub(crate) fn reply_for(frame: &[&[u8]]) -> Vec<u8> {
    let args = frame[1..].iter().map(|arg| arg.to_vec()).collect();
    match Command::parse(frame[0], args) {
        Ok(command) => command.execute(),
        Err(reply) => reply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_message_depends_on_args() {
        assert_eq!(
            reply_for(&[b"garbage"]),
            b"-ERR unknown command 'garbage'\r\n"
        );
        assert_eq!(
            reply_for(&[b"NOPE", b"a", b"b"]),
            b"-ERR unknown command 'NOPE', with args beginning with: 'a' 'b' \r\n"
        );
    }
}
