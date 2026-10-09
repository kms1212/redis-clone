use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::{
    command::Command,
    db::Db,
    error::Error,
    resp::{Limits, read_array_len, read_bulk},
};

/// Enough for almost every command; larger arrays just grow as usual.
const MAX_PREALLOCATED_ARGS: usize = 16;

// Accepts any readable and writable stream, not just TcpStream, so tests can pass tokio::io::duplex.
pub(crate) async fn handle_connection<S>(stream: S, limits: Limits, db: Db) -> Result<(), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(stream);
    let result = serve(&mut reader, limits, &db).await;
    if let Err(Error::Protocol(error)) = &result {
        // Like real Redis: tell the client what was wrong, then close the connection.
        reader.get_mut().write_all(&error.reply().encode()).await?;
    }
    result
}

/// Reads and answers commands until the client disconnects or sends something malformed.
async fn serve<S>(reader: &mut BufReader<S>, limits: Limits, db: &Db) -> Result<(), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    while let Some(count) = read_array_len(reader, limits).await? {
        // Real Redis sends no reply to an empty array (`*0\r\n`) either.
        if count == 0 {
            continue;
        }
        let name = read_bulk(reader, limits).await?;

        // Capped: trusting count alone would let a single `*999999999` line reserve a lot of memory.
        let mut args = Vec::with_capacity((count - 1).min(MAX_PREALLOCATED_ARGS));
        for _ in 1..count {
            args.push(read_bulk(reader, limits).await?);
        }

        let reply = match Command::parse(&name, args) {
            // The guard lives only inside this arm, so the lock is released before the reply
            // is written below.
            Ok(command) => command.execute(&mut db.lock()),
            Err(reply) => reply,
        };
        reader.get_mut().write_all(&reply.encode()).await?;
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
        tokio::spawn(handle_connection(
            server,
            Limits {
                message_bytes: 1024,
                header_bytes: 64,
            },
            Db::default(),
        ));
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

    #[tokio::test]
    async fn protocol_error_is_reported_then_connection_closes() {
        let mut client = connect();
        client
            .write_all(b"*1\r\n$4\r\nPING\r\n*1\r\n+PING\r\n*1\r\n$4\r\nPING\r\n")
            .await
            .unwrap();

        // Reading to the end only finishes if the server closes the connection.
        let mut replies = Vec::new();
        timeout(Duration::from_secs(1), client.read_to_end(&mut replies))
            .await
            .expect("server should close the connection")
            .unwrap();
        // The command before the error is answered; the one after it is not.
        assert_eq!(
            replies,
            b"+PONG\r\n-ERR Protocol error: expected '$', got '+'\r\n"
        );
    }
}
