use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::{
    command::Command,
    error::Error,
    resp::{read_bulk, read_length},
};

// Accepts any readable and writable stream, not just TcpStream, so tests can pass tokio::io::duplex.
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
        // No with_capacity(count): trusting count would let a single `*999999999` line reserve a lot of memory.
        let mut frame = Vec::new();
        for _ in 0..count {
            frame.push(read_bulk(&mut reader, max_message_bytes, max_header_bytes).await?);
        }

        // Real Redis sends no reply to an empty array (`*0\r\n`) either.
        if frame.is_empty() {
            continue;
        }
        // Takes ownership of the first element. The rest only shift their pointers; no bytes are copied.
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

    /// Attaches the server to an in-memory fake socket and returns the client end.
    fn connect() -> DuplexStream {
        let (client, server) = duplex(4096);
        tokio::spawn(handle_connection(server, 1024, 64));
        client
    }

    /// Fails after 1 second so a missing reply does not hang the test forever.
    async fn read_reply(client: &mut DuplexStream, len: usize) -> Vec<u8> {
        let mut reply = vec![0; len];
        timeout(Duration::from_secs(1), client.read_exact(&mut reply))
            .await
            .expect("reply should arrive within 1 second")
            .unwrap();
        reply
    }

    #[tokio::test]
    async fn replies_to_complete_commands_before_the_rest_arrives() {
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
    async fn empty_array_gets_no_reply() {
        let mut client = connect();
        client
            .write_all(b"*0\r\n*1\r\n$4\r\nPING\r\n")
            .await
            .unwrap();
        // If `*0` had produced a reply, it would arrive before PONG.
        assert_eq!(read_reply(&mut client, 7).await, b"+PONG\r\n");
    }
}
