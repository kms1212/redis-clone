mod ping;

use crate::resp::error_reply;

/// 파싱이 끝난 커맨드입니다. 인자 검사는 `parse` 에서 끝나므로, 여기 담긴 값은 항상 실행할 수 있습니다.
pub(crate) enum Command {
    Ping(Option<Vec<u8>>),
}

impl Command {
    /// 실패하면 클라이언트에게 그대로 보낼 에러 응답을 돌려줍니다.
    pub(crate) fn parse(name: &[u8], args: Vec<Vec<u8>>) -> Result<Self, Vec<u8>> {
        if name.eq_ignore_ascii_case(b"PING") {
            return ping::parse(args);
        }

        Err(unknown_command_reply(name, &args))
    }

    // self 를 소비합니다. 담긴 인자를 복사하지 않고 응답으로 넘길 수 있습니다.
    pub(crate) fn execute(self) -> Vec<u8> {
        match self {
            Self::Ping(message) => ping::execute(message),
        }
    }
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

/// 테스트에서 "이 바이트들을 보내면 무슨 응답이 나오나"를 한 줄로 쓰기 위한 도우미입니다.
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
    fn 모르는_커맨드는_인자_유무에_따라_문구가_다르다() {
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
