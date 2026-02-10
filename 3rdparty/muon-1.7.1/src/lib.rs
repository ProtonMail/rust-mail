//! # Muon
//!
//! Muon (named like the particle) is a client library for the Proton REST API.
//!
//! ## Usage
//!
//! The `muon` crate is published to the internal Proton registry. Configure
//! cargo to use the Proton registry and add `muon` to your project.
//!
//! ## Create a client
//!
//! A client needs at least an [`App`] and something implementing a
//! [`store::Store`] (see [`muon::store`](`crate::store`) for more information)
//!
//! ### Example
//! ```
//! # use muon::doc::*;
//! use muon::client::Auth;
//! use muon::{App, Client, GET};
//! # tokio_test::block_on(async {
//! let store = MyPersistenceStorage::prod();
//! let app = App::new("windows-vpn@4.1.0")?;
//! let client = Client::new(app, store)?;
//! let res = client.send(GET!("/tests/ping")).await?;
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! ## Examples
//!
//! A variety of examples demonstrating specific features of the library can be
//! found in the `examples` directory.

#[macro_use]
extern crate tracing;

#[macro_use]
mod macros;

#[macro_use]
mod cfg;

pub mod app;
pub use app::App;

mod auth;
pub(crate) use auth::*;

pub mod client;
pub(crate) use client::*;
pub use client::{headers, Client};

pub mod env;

pub mod error;
pub(crate) use error::*;
pub use error::{Error, Result};

pub mod http;
pub use http::{
    serde_to_query, ContentType, HttpReq as ProtonRequest, HttpRes as ProtonResponse,
    HttpSender as ProtonSender, Method, Status, StatusErr, Version,
};

pub mod store;

/// Module containing the optional utils provided by muon
#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "ffi")]
pub use muon_proc::driver;
#[cfg(feature = "util")]
pub use muon_proc::{autoimpl, derive_dyn};
#[cfg(feature = "testing")]
pub use muon_proc::{main, test};

#[cfg(feature = "testing")]
pub mod test;

/// Re-export serde-json for downstream convenience.
pub use serde_json as json;
pub mod common;
pub mod deps;
pub mod dns;

#[cfg(feature = "doctest")]
pub mod doc;

export! {
    mod private;
}

pub mod rest;
pub mod rt;
pub mod tls;
