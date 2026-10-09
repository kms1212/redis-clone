use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::TcpStream,
};

use crate::{
    command::build_reply,
    error::Error,
    resp::{read_bulk, read_length},
};

pub(crate) async fn handle_connection(
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
