use crate::http::Status;
use crate::rest::ApiErr;
use axum::response::{IntoResponse, Response};
use axum::Json;
use thiserror::Error;

/// A server result.
pub type ServerRes<T> = Result<T, ServerErr>;

/// An error, either client-side or server-side.
#[derive(Debug, Error)]
pub enum ServerErr {
    #[error("client error")]
    Client(Status, ApiErr),

    #[error("server error")]
    Server(anyhow::Error),
}

impl ServerErr {
    /// Creates a new client error>
    pub fn client(status: Status, code: u16, error: impl AsRef<str>) -> Self {
        let error = error.as_ref().to_owned();

        Self::Client(status, ApiErr { code, error })
    }
}

impl IntoResponse for ServerErr {
    fn into_response(self) -> Response {
        match self {
            Self::Client(status, err) => {
                let res = (status, Json(err));
                res.into_response()
            }

            Self::Server(err) => {
                let res = (Status::INTERNAL_SERVER_ERROR, format!("oops: {err}"));
                res.into_response()
            }
        }
    }
}

/// Creates a new client error.
#[macro_export]
macro_rules! cerr {
    ($status:expr, $($tt:tt)*) => {{
        use $crate::test::server::error::ServerErr;

        let Ok(status) = $status.try_into() else {
            panic!("invalid status: {}", $status);
        };

        ServerErr::client(status, 9001, format!($($tt)*))
    }};
}

/// Creates a new server error.
#[macro_export]
macro_rules! serr {
    ($($tt:tt)*) => {{
        use $crate::test::server::error::ServerErr;

        ServerErr::Server(anyhow::anyhow!($($tt)*))
    }};
}

pub(crate) use {cerr, serr};
