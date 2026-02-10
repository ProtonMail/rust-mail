//! ## TLS
//!
//! This module provides TLS types and backends for the client.

export! {
    mod common (as pub);
}

if_not_wasm! {
    if_tls_rustls! {
        export! { mod rustls (as pub); }
    }

    if_tls_tokio! {
        export! { mod tokio (as pub); }
    }
}

#[cfg(test)]
mod tests;
