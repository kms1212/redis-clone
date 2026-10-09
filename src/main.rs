mod command;
mod connection;
mod error;
mod resp;

use std::{env, net::IpAddr};

use tokio::net::TcpListener;

use crate::{connection::handle_connection, error::Error, resp::Limits};

struct Config {
    bind: IpAddr,
    port: u16,
    limits: Limits,
}

impl Config {
    fn from_args() -> Result<Self, Error> {
        let mut config = Self {
            bind: IpAddr::from([127, 0, 0, 1]),
            port: 6380,
            limits: Limits {
                message_bytes: 1_048_576,
                header_bytes: 64,
            },
        };

        let mut args = env::args().skip(1);
        while let Some(option) = args.next() {
            let value = args.next().ok_or(Error::InvalidRequest)?;
            match option.as_str() {
                "--bind" => config.bind = value.parse().map_err(|_| Error::InvalidRequest)?,
                "--port" => config.port = value.parse().map_err(|_| Error::InvalidRequest)?,
                "--max-message-bytes" => {
                    config.limits.message_bytes =
                        value.parse().map_err(|_| Error::InvalidRequest)?;
                }
                "--max-header-bytes" => {
                    config.limits.header_bytes =
                        value.parse().map_err(|_| Error::InvalidRequest)?;
                }
                _ => return Err(Error::InvalidRequest),
            }
        }

        if config.limits.header_bytes == 0 {
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
                eprintln!("failed to accept connection: {error}");
                continue;
            }
        };

        let limits = config.limits;
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, limits).await {
                eprintln!("{peer} disconnected: {error}");
            }
        });
    }
}
