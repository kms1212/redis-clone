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
