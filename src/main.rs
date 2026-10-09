use std::{env, fmt, io, net::IpAddr};

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

#[derive(Debug)]
enum Error {
    Io(io::Error),
    InvalidRequest,
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidRequest => write!(formatter, "잘못된 입력입니다"),
        }
    }
}

struct Config {
    bind: IpAddr,
    port: u16,
    max_message_bytes: usize,
    max_header_bytes: usize,
}

impl Config {
    fn from_args() -> Result<Self, Error> {
        let mut config = Self {
            bind: IpAddr::from([127, 0, 0, 1]),
            port: 6380,
            max_message_bytes: 1_048_576,
            max_header_bytes: 64,
        };

        let mut args = env::args().skip(1);
        while let Some(option) = args.next() {
            let value = args.next().ok_or(Error::InvalidRequest)?;
            match option.as_str() {
                "--bind" => config.bind = value.parse().map_err(|_| Error::InvalidRequest)?,
                "--port" => config.port = value.parse().map_err(|_| Error::InvalidRequest)?,
                "--max-message-bytes" => {
                    config.max_message_bytes = value.parse().map_err(|_| Error::InvalidRequest)?;
                }
                "--max-header-bytes" => {
                    config.max_header_bytes = value.parse().map_err(|_| Error::InvalidRequest)?;
                }
                _ => return Err(Error::InvalidRequest),
            }
        }

        if config.max_header_bytes == 0 {
            return Err(Error::InvalidRequest);
        }
        Ok(config)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let config = Config::from_args()?;
    let listener = TcpListener::bind((config.bind, config.port)).await?;

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("접속을 받지 못했습니다: {error}");
                continue;
            }
        };

        let max_message_bytes = config.max_message_bytes;
        let max_header_bytes = config.max_header_bytes;
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, max_message_bytes, max_header_bytes).await
            {
                eprintln!("{peer} 연결 종료: {error}");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    max_message_bytes: usize,
    max_header_bytes: usize,
) -> Result<(), Error> {
    let mut reader = BufReader::new(stream);

    while let Some(count) = read_length(&mut reader, b'*', max_header_bytes).await? {
        // count 를 믿고 with_capacity 를 쓰면 `*999999999` 한 줄로 메모리를 크게 잡을 수 있습니다.
        let mut command = Vec::new();
        for _ in 0..count {
            command.push(read_bulk(&mut reader, max_message_bytes, max_header_bytes).await?);
        }

        let reply = build_reply(&command);
        // 빈 배열(`*0\r\n`)에는 진짜 Redis도 아무 응답을 하지 않습니다.
        if !reply.is_empty() {
            reader.get_mut().write_all(&reply).await?;
        }
    }
    Ok(())
}

fn build_reply(command: &[Vec<u8>]) -> Vec<u8> {
    let Some(name) = command.first() else {
        return Vec::new();
    };

    if name.eq_ignore_ascii_case(b"PING") {
        return match command.len() {
            1 => b"+PONG\r\n".to_vec(),
            2 => bulk_string(&command[1]),
            _ => error_reply(b"ERR wrong number of arguments for 'ping' command"),
        };
    }

    unknown_command_reply(name, &command[1..])
}

fn bulk_string(value: &[u8]) -> Vec<u8> {
    let mut reply = format!("${}\r\n", value.len()).into_bytes();
    reply.extend_from_slice(value);
    reply.extend_from_slice(b"\r\n");
    reply
}

fn error_reply(message: &[u8]) -> Vec<u8> {
    let mut reply = vec![b'-'];
    reply.extend_from_slice(message);
    reply.extend_from_slice(b"\r\n");
    reply
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

async fn read_length(
    reader: &mut BufReader<TcpStream>,
    prefix: u8,
    max_header_bytes: usize,
) -> Result<Option<usize>, Error> {
    let mut header = Vec::new();
    let bytes_read = reader
        .take(max_header_bytes as u64)
        .read_until(b'\n', &mut header)
        .await?;
    if bytes_read == 0 {
        return Ok(None);
    }
    if header.len() < 4 || header[0] != prefix || !header.ends_with(b"\r\n") {
        return Err(Error::InvalidRequest);
    }

    let digits = &header[1..header.len() - 2];
    if !digits.iter().all(u8::is_ascii_digit) {
        return Err(Error::InvalidRequest);
    }
    let length = std::str::from_utf8(digits)
        .map_err(|_| Error::InvalidRequest)?
        .parse()
        .map_err(|_| Error::InvalidRequest)?;
    Ok(Some(length))
}

async fn read_bulk(
    reader: &mut BufReader<TcpStream>,
    max_bytes: usize,
    max_header_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let length = read_length(reader, b'$', max_header_bytes)
        .await?
        .ok_or(Error::InvalidRequest)?;
    if length > max_bytes {
        return Err(Error::InvalidRequest);
    }

    let mut data = Vec::new();
    data.try_reserve_exact(length)
        .map_err(|_| Error::InvalidRequest)?;
    data.resize(length, 0);
    reader.read_exact(&mut data).await?;
    let mut terminator = [0; 2];
    reader.read_exact(&mut terminator).await?;
    if terminator != *b"\r\n" {
        return Err(Error::InvalidRequest);
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_응답이_진짜_redis와_같다() {
        assert_eq!(build_reply(&[b"PiNg".to_vec()]), b"+PONG\r\n");
        assert_eq!(
            build_reply(&[b"PING".to_vec(), "한글".into()]),
            "$6\r\n한글\r\n".as_bytes()
        );
        assert_eq!(
            build_reply(&[b"PING".to_vec(), b"a".to_vec(), b"b".to_vec()]),
            b"-ERR wrong number of arguments for 'ping' command\r\n"
        );
    }

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
