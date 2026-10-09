use super::Command;
use crate::resp::{bulk_string, error_reply};

/// `args` 는 커맨드 이름(`PING`)을 뺀 나머지입니다.
pub(super) fn parse(args: Vec<Vec<u8>>) -> Result<Command, Vec<u8>> {
    // into_iter 로 꺼내면 인자를 복사하지 않고 소유권째 가져옵니다.
    let mut args = args.into_iter();
    match (args.next(), args.next()) {
        (None, _) => Ok(Command::Ping(None)),
        (Some(message), None) => Ok(Command::Ping(Some(message))),
        _ => Err(error_reply(
            b"ERR wrong number of arguments for 'ping' command",
        )),
    }
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

    #[test]
    fn ping_응답이_진짜_redis와_같다() {
        assert_eq!(reply_for(&[b"PiNg"]), b"+PONG\r\n");
        assert_eq!(
            reply_for(&[b"PING", "한글".as_bytes()]),
            "$6\r\n한글\r\n".as_bytes()
        );
        assert_eq!(
            reply_for(&[b"PING", b"a", b"b"]),
            b"-ERR wrong number of arguments for 'ping' command\r\n"
        );
    }
}
