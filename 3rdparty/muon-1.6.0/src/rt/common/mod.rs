//! ## Common runtime traits and types
//!
//! This module provides runtime-agnostic traits for the various runtime
//! components used by Muon. Concrete implementations are enabled via feature
//! flags, such as `rt-async` and `rt-tokio`.

export! {
    mod dialer (as pub);
    mod dispatcher (as pub);
    mod resolver (as pub);
    mod spawner (as pub);
}
