use tokio::io::AsyncBufRead;

use super::Command;
use crate::{
    error::Error,
    resp::{Args, bulk_string, error_reply},
};

/// `args` excludes the command name (`ECHO`). ECHO takes exactly one argument.
pub(super) async fn parse<R: AsyncBufRead + Unpin>(
    args: &mut Args<'_, R>,
) -> Result<Result<Command, Vec<u8>>, Error> {
    if args.remaining() == 1
        && let Some(message) = args.next().await?
    {
        return Ok(Ok(Command::Echo(message)));
    }
    // Arity is known from the array header, but the arguments must still be consumed.
    args.rest().await?;
    Ok(Err(error_reply(
        b"ERR wrong number of arguments for 'echo' command",
    )))
}

pub(super) fn execute(message: Vec<u8>) -> Vec<u8> {
    bulk_string(&message)
}

#[cfg(test)]
mod tests {
    use crate::command::reply_for;

    #[tokio::test]
    async fn echo_replies_match_real_redis() {
        assert_eq!(reply_for(&[b"ECHO", b"hello"]).await, b"$5\r\nhello\r\n");
        assert_eq!(reply_for(&[b"EcHo", b"hello"]).await, b"$5\r\nhello\r\n");
        // Length is counted in bytes: two Hangul syllables are 6 bytes.
        assert_eq!(
            reply_for(&[b"ECHO", "\u{d55c}\u{ae00}".as_bytes()]).await,
            "$6\r\n\u{d55c}\u{ae00}\r\n".as_bytes()
        );
        // difftest cannot send an empty argument, so it is checked here.
        assert_eq!(reply_for(&[b"ECHO", b""]).await, b"$0\r\n\r\n");
    }

    #[tokio::test]
    async fn echo_needs_exactly_one_argument() {
        let wrong = b"-ERR wrong number of arguments for 'echo' command\r\n";
        assert_eq!(reply_for(&[b"ECHO"]).await, wrong);
        assert_eq!(reply_for(&[b"ECHO", b"a", b"b"]).await, wrong);
    }
}
