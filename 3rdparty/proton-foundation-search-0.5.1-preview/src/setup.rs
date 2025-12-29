//! # Helper Functions for setting up a WASM environment
//!
//! Utility functions for configuring the WebAssembly environment, including:
//! - Panic hook setup for better error reporting in browser console
//! - Tracing configuration for debugging and performance monitoring
//! - Web-specific logging setup

use wasm_bindgen::prelude::wasm_bindgen;

/// Handle panics nicely for the web console
#[wasm_bindgen(js_name = "setPanicHook")]
pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    console_error_panic_hook::set_once();
}

/// Log to web console
#[wasm_bindgen(js_name = "enableTracing")]
pub fn enable_tracing() {
    use tracing_subscriber::fmt::format::Pretty;
    use tracing_subscriber::prelude::*;
    use tracing_web::{MakeWebConsoleWriter, performance_layer};

    let fmt_layer = tracing_subscriber::fmt::layer()
        // Only partially supported across browsers
        .with_ansi(false)
        // std::time is not available in browsers, can be set, see the tracing_web doc
        .without_time()
        // write events to the console
        .with_writer(MakeWebConsoleWriter::new().with_pretty_level());
    let perf_layer = performance_layer().with_details_from_fields(Pretty::default());

    if let Err(err) = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(perf_layer)
        // Install these as subscribers to tracing events
        .try_init()
    {
        tracing::error!(message = "unable to set tracing", cause = %err);
    }
}
