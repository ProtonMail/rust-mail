//! ## TLS
//!
//! This module provides TLS types and backends for the client.

export! {
    mod common (as pub);
}

crate::if_not_wasm! {
    crate::if_tls_rustls! {
        export! { mod rustls (as pub); }
    }

    crate::if_tls_tokio! {
        export! { mod tokio (as pub); }
    }
}

#[cfg(test)]
mod tests;
