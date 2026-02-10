//! ## Runtime
//!
//! This module provides implementations of runtime-dependent components, such
//! as the resolver, dialer and executor.

export! {
    mod common (as pub);
}

if_not_wasm! {
    if_rt_async! {
        export! { mod r#async (as pub); }
    }

    if_rt_tokio! {
        export! { mod tokio (as pub); }
    }
}
