use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::{
    command::Command,
    error::Error,
    resp::{read_bulk, read_length},
};

// TcpStream 대신 "읽고 쓸 수 있는 무엇이든"을 받습니다. 테스트에서는 tokio::io::duplex 를 넘깁니다.
pub(crate) async fn handle_connection<S>(
    stream: S,
    max_message_bytes: usize,
    max_header_bytes: usize,
) -> Result<(), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(stream);

    while let Some(count) = read_length(&mut reader, b'*', max_header_bytes).await? {
        // count 를 믿고 with_capacity 를 쓰면 `*999999999` 한 줄로 메모리를 크게 잡을 수 있습니다.
        let mut frame = Vec::new();
        for _ in 0..count {
            frame.push(read_bulk(&mut reader, max_message_bytes, max_header_bytes).await?);
        }

        // 빈 배열(`*0\r\n`)에는 진짜 Redis도 아무 응답을 하지 않습니다.
        if frame.is_empty() {
            continue;
        }
        // 첫 원소를 소유권째 꺼냅니다. 나머지 원소는 포인터만 한 칸씩 당겨지고, 바이트는 복사되지 않습니다.
        let name = frame.remove(0);

        let reply = match Command::parse(&name, frame) {
            Ok(command) => command.execute(),
            Err(reply) => reply,
        };
        reader.get_mut().write_all(&reply).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt, DuplexStream, duplex},
        time::timeout,
    };

    use super::*;

    /// 서버를 메모리 안의 가짜 소켓에 붙이고, 클라이언트 쪽 끝을 돌려줍니다.
    fn connect() -> DuplexStream {
        let (client, server) = duplex(4096);
        tokio::spawn(handle_connection(server, 1024, 64));
        client
    }

    /// 응답이 안 오면 테스트가 영원히 멈추지 않도록 1초 안에 못 받으면 실패시킵니다.
    async fn read_reply(client: &mut DuplexStream, len: usize) -> Vec<u8> {
        let mut reply = vec![0; len];
        timeout(Duration::from_secs(1), client.read_exact(&mut reply))
            .await
            .expect("1초 안에 응답이 와야 합니다")
            .unwrap();
        reply
    }

    #[tokio::test]
    async fn 완결된_명령은_뒤_명령이_덜_와도_먼저_응답한다() {
        let mut client = connect();
        client
            .write_all(b"*1\r\n$4\r\nPING\r\n*2\r\n$4\r\nPING\r\n$2\r\nhi\r\n*2\r\n$4\r\nPING\r\n$5\r\nwor")
            .await
            .unwrap();
        assert_eq!(read_reply(&mut client, 15).await, b"+PONG\r\n$2\r\nhi\r\n");

        client.write_all(b"ld\r\n").await.unwrap();
        assert_eq!(read_reply(&mut client, 11).await, b"$5\r\nworld\r\n");
    }

    #[tokio::test]
    async fn 빈_배열에는_응답하지_않는다() {
        let mut client = connect();
        client
            .write_all(b"*0\r\n*1\r\n$4\r\nPING\r\n")
            .await
            .unwrap();
        // `*0` 에 무언가 응답했다면 PONG 보다 먼저 왔을 것입니다.
        assert_eq!(read_reply(&mut client, 7).await, b"+PONG\r\n");
    }
}
