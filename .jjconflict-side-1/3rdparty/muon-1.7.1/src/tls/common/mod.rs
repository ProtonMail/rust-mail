//! ## Common TLS traits and types
//!
//! This module provides common TLS types and traits that are used by the
//! various TLS backends in Muon. Concrete implementations are enabled via
//! feature flags, such as `tls-rustls` and `tls-tokio`.

export! {
    mod backend (as pub);
    mod anchor (as pub);
    mod alpn (as pub);
    mod certs (as pub);
    mod pins (as pub);
    mod upgrader (as pub);
    mod util (as pub);
    mod verifier (as pub);
}
