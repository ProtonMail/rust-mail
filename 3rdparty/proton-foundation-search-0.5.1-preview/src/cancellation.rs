//! A cancellation token implementation
//! We may remove this as it isn't really part of the search solution.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A token of cancellation
#[derive(Debug, Default)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct CancellationToken {
    cancel: Arc<AtomicBool>,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl CancellationToken {
    /// Create a new token
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]

    pub fn new() -> Self {
        Self::default()
    }

    /// Check if it is cancelled
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgenwasm_bindgen(js_name = "isCancelled")
    )]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    /// Create a guard that will cancel on drop or explicitly
    pub fn guard(&self) -> CancellationGuard {
        CancellationGuard {
            cancel: self.cancel.clone(),
        }
    }
}

/// Cancellation guard will cancel on drop
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct CancellationGuard {
    cancel: Arc<AtomicBool>,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl CancellationGuard {
    /// Check if it is cancelled
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "isCancelled")
    )]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    /// Cancel the token explicitly
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for CancellationGuard {
    fn drop(&mut self) {
        self.cancel()
    }
}
