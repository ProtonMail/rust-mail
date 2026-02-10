//! ## HTTP Macros
//!
//! This module provides macros for building HTTP requests.

/// Create a new `GET` request.
#[macro_export]
macro_rules! GET {
    ($($tt:tt)*) => {{
        $crate::http::HttpReq::new($crate::http::Method::GET, format!($($tt)*))
    }}
}

/// Create a new `POST` request.
#[macro_export]
macro_rules! POST {
    ($($tt:tt)*) => {{
        $crate::http::HttpReq::new($crate::http::Method::POST, format!($($tt)*))
    }}
}

/// Create a new `PUT` request.
#[macro_export]
macro_rules! PUT {
    ($($tt:tt)*) => {{
        $crate::http::HttpReq::new($crate::http::Method::PUT, format!($($tt)*))
    }}
}

/// Create a new `DELETE` request.
#[macro_export]
macro_rules! DELETE {
    ($($tt:tt)*) => {{
        $crate::http::HttpReq::new($crate::http::Method::DELETE, format!($($tt)*))
    }}
}

/// Create a new `PATCH` request.
#[macro_export]
macro_rules! PATCH {
    ($($tt:tt)*) => {{
        $crate::http::HttpReq::new($crate::http::Method::PATCH, format!($($tt)*))
    }}
}

/// Re-export for better name resolution inside the crate.
pub use {DELETE, GET, PATCH, POST, PUT};
