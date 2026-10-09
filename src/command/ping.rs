use tokio::io::AsyncBufRead;

use super::Command;
use crate::{
    error::Error,
    resp::{Args, bulk_string, error_reply},
};

/// `args` excludes the command name (`PING`).
pub(super) async fn parse<R: AsyncBufRead + Unpin>(
    args: &mut Args<'_, R>,
) -> Result<Result<Command, Vec<u8>>, Error> {
    // Arity is known from the array header, but the arguments must still be consumed.
    if args.remaining() > 1 {
        args.rest().await?;
        return Ok(Err(error_reply(
            b"ERR wrong number of arguments for 'ping' command",
        )));
    }
    Ok(Ok(Command::Ping(args.next().await?)))
}

pub(super) fn execute(message: Option<Vec<u8>>) -> Vec<u8> {
    match message {
        None => b"+PONG\r\n".to_vec(),
        Some(message) => bulk_string(&message),
    }
}

#[cfg(test)]
mod tests {
    use crate::command::reply_for;

    #[tokio::test]
    async fn ping_replies_match_real_redis() {
        assert_eq!(reply_for(&[b"PiNg"]).await, b"+PONG\r\n");
        assert_eq!(
            reply_for(&[b"PING", "\u{d55c}\u{ae00}".as_bytes()]).await,
            "$6\r\n\u{d55c}\u{ae00}\r\n".as_bytes()
        );
        assert_eq!(
            reply_for(&[b"PING", b"a", b"b"]).await,
            b"-ERR wrong number of arguments for 'ping' command\r\n"
        );
    }
}
