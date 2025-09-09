//! TLS implementation using `rustls`.

export! {
    mod backend (as pub);
    mod upgrader (as pub);
}

crate::if_android! {
    export! {
            mod android (as pub);
    }
}
