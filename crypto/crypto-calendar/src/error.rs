use std::{error, fmt, result};

pub type Result<T, E = Error> = result::Result<T, E>;

#[derive(Debug)]
pub struct Error(anyhow::Error);

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl error::Error for Error {
    //
}
