use std::{fmt, io};

#[derive(Debug)]
pub(crate) enum Error {
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
