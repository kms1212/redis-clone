use crate::resp::{bulk_string, error_reply};

/// `args` 는 커맨드 이름(`PING`)을 뺀 나머지입니다.
pub(crate) fn reply(args: &[Vec<u8>]) -> Vec<u8> {
    match args {
        [] => b"+PONG\r\n".to_vec(),
        [message] => bulk_string(message),
        _ => error_reply(b"ERR wrong number of arguments for 'ping' command"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::build_reply;

    #[test]
    fn ping_응답이_진짜_redis와_같다() {
        assert_eq!(build_reply(&[b"PiNg".to_vec()]), b"+PONG\r\n");
        assert_eq!(reply(&["한글".into()]), "$6\r\n한글\r\n".as_bytes());
        assert_eq!(
            reply(&[b"a".to_vec(), b"b".to_vec()]),
            b"-ERR wrong number of arguments for 'ping' command\r\n"
        );
    }
}
