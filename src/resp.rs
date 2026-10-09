use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    net::TcpStream,
};

use crate::error::Error;

pub(crate) async fn read_length(
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

pub(crate) async fn read_bulk(
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

pub(crate) fn bulk_string(value: &[u8]) -> Vec<u8> {
    let mut reply = format!("${}\r\n", value.len()).into_bytes();
    reply.extend_from_slice(value);
    reply.extend_from_slice(b"\r\n");
    reply
}

pub(crate) fn error_reply(message: &[u8]) -> Vec<u8> {
    let mut reply = vec![b'-'];
    reply.extend_from_slice(message);
    reply.extend_from_slice(b"\r\n");
    reply
}
