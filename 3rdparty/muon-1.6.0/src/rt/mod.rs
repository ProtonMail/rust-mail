//! ## Runtime
//!
//! This module provides implementations of runtime-dependent components, such
//! as the resolver, dialer and executor.

crate::export! {
    mod common (as pub);
}

crate::if_not_wasm! {
    crate::if_rt_async! {
        export! { mod r#async (as pub); }
    }

    crate::if_rt_tokio! {
        export! { mod tokio (as pub); }
    }
}
