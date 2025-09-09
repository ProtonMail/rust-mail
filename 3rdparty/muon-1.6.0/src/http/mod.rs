//! ## Muon Http
//!
//! This module defines the HTTP implementation(s) for Muon.

crate::export! {
    mod common (as pub);
}

crate::if_wasm! {{
        crate::export! { mod reqwest (as pub); }
} else {
        crate::export! { mod hyper (as pub); }
}}
