use super::{Command, wrong_args_reply};
use crate::resp::bulk_string;

/// `args` excludes the command name (`ECHO`). ECHO takes exactly one argument.
pub(super) fn parse(args: Vec<Vec<u8>>) -> Result<Command, Vec<u8>> {
    let mut args = args.into_iter();
    match (args.next(), args.next()) {
        (Some(message), None) => Ok(Command::Echo(message)),
        _ => Err(wrong_args_reply("echo")),
    }
}

pub(super) fn execute(message: Vec<u8>) -> Vec<u8> {
    bulk_string(&message)
}

#[cfg(test)]
mod tests {
    use crate::command::reply_for;

    #[test]
    fn echo_replies_match_real_redis() {
        assert_eq!(reply_for(&[b"ECHO", b"hello"]), b"$5\r\nhello\r\n");
        assert_eq!(reply_for(&[b"EcHo", b"hello"]), b"$5\r\nhello\r\n");
        // Length is counted in bytes: two Hangul syllables are 6 bytes.
        assert_eq!(
            reply_for(&[b"ECHO", "\u{d55c}\u{ae00}".as_bytes()]),
            "$6\r\n\u{d55c}\u{ae00}\r\n".as_bytes()
        );
        // difftest cannot send an empty argument, so it is checked here.
        assert_eq!(reply_for(&[b"ECHO", b""]), b"$0\r\n\r\n");
    }

    #[test]
    fn echo_needs_exactly_one_argument() {
        let wrong = b"-ERR wrong number of arguments for 'echo' command\r\n";
        assert_eq!(reply_for(&[b"ECHO"]), wrong);
        assert_eq!(reply_for(&[b"ECHO", b"a", b"b"]), wrong);
    }
}
