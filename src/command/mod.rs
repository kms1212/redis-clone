mod ping;

use crate::resp::error_reply;

pub(crate) fn build_reply(command: &[Vec<u8>]) -> Vec<u8> {
    let Some(name) = command.first() else {
        return Vec::new();
    };
    let args = &command[1..];

    if name.eq_ignore_ascii_case(b"PING") {
        return ping::reply(args);
    }

    unknown_command_reply(name, args)
}

fn unknown_command_reply(name: &[u8], args: &[Vec<u8>]) -> Vec<u8> {
    // String 을 거치지 않고 바이트로 이어 붙입니다. UTF-8 이 아닌 이름도 받은 그대로 돌려줘야 해서입니다.
    let mut message = b"ERR unknown command '".to_vec();
    message.extend_from_slice(name);
    message.push(b'\'');
    if !args.is_empty() {
        message.extend_from_slice(b", with args beginning with: ");
        for arg in args {
            // 진짜 Redis는 인자마다 뒤에 공백을 하나씩 붙입니다. 마지막 인자 뒤에도 붙습니다.
            message.push(b'\'');
            message.extend_from_slice(arg);
            message.extend_from_slice(b"' ");
        }
    }
    error_reply(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 모르는_커맨드는_인자_유무에_따라_문구가_다르다() {
        assert_eq!(
            build_reply(&[b"garbage".to_vec()]),
            b"-ERR unknown command 'garbage'\r\n"
        );
        assert_eq!(
            build_reply(&[b"NOPE".to_vec(), b"a".to_vec(), b"b".to_vec()]),
            b"-ERR unknown command 'NOPE', with args beginning with: 'a' 'b' \r\n"
        );
    }

    #[test]
    fn 빈_배열에는_응답하지_않는다() {
        assert!(build_reply(&[]).is_empty());
    }
}
