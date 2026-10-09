mod echo;
mod ping;

use tokio::io::AsyncBufRead;

use crate::{
    error::Error,
    resp::{Args, error_reply},
};

/// A parsed command. Arguments are validated in `parse`, so every value here can be executed as is.
pub(crate) enum Command {
    Ping(Option<Vec<u8>>),
    Echo(Vec<u8>),
}

impl Command {
    /// The outer error means the stream itself failed. The inner `Err` is the error reply
    /// to send back to the client unchanged.
    pub(crate) async fn parse<R: AsyncBufRead + Unpin>(
        name: &[u8],
        args: &mut Args<'_, R>,
    ) -> Result<Result<Self, Vec<u8>>, Error> {
        if name.eq_ignore_ascii_case(b"PING") {
            return ping::parse(args).await;
        }
        if name.eq_ignore_ascii_case(b"ECHO") {
            return echo::parse(args).await;
        }

        let rest = args.rest().await?;
        Ok(Err(unknown_command_reply(name, &rest)))
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
pub(crate) async fn reply_for(frame: &[&[u8]]) -> Vec<u8> {
    // Commands now read from a stream, so the arguments are encoded back into RESP first.
    let mut encoded = Vec::new();
    for arg in &frame[1..] {
        encoded.extend_from_slice(&crate::resp::bulk_string(arg));
    }
    let mut input = &encoded[..];
    let mut args = Args::new(&mut input, frame.len() - 1, 1024, 64);
    match Command::parse(frame[0], &mut args).await.unwrap() {
        Ok(command) => command.execute(),
        Err(reply) => reply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_command_message_depends_on_args() {
        assert_eq!(
            reply_for(&[b"garbage"]).await,
            b"-ERR unknown command 'garbage'\r\n"
        );
        assert_eq!(
            reply_for(&[b"NOPE", b"a", b"b"]).await,
            b"-ERR unknown command 'NOPE', with args beginning with: 'a' 'b' \r\n"
        );
    }
}
