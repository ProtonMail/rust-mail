//! ## Muon Http
//!
//! This module defines the HTTP implementation(s) for Muon.

export! {
    mod common (as pub);
}

if_wasm! {{
        export! { mod reqwest (as pub); }
} else {
        export! { mod hyper (as pub); }
}}
