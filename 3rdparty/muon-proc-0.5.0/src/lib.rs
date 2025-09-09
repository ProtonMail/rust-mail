//! # Muon Proc
//!
//! This crate implements proc macros for the `muon` crate.

use proc_macro::TokenStream;

/// Internal implementation details.
mod internal;

/// Automatically derive a blanket implementation for a trait, if possible.
#[proc_macro_attribute]
pub fn autoimpl(args: TokenStream, item: TokenStream) -> TokenStream {
    internal::autoimpl(args, item).into()
}

/// Links an async function to be polled with a future.
#[proc_macro_attribute]
pub fn driver(args: TokenStream, item: TokenStream) -> TokenStream {
    internal::driver(args, item).into()
}

/// Derive various traits for a `dyn Trait` type.
#[proc_macro_attribute]
pub fn derive_dyn(args: TokenStream, item: TokenStream) -> TokenStream {
    internal::derive_dyn(args, item).into()
}

/// Expand the `test` macro.
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    internal::runner_test(args, item).into()
}

/// Expand the `main` macro.
#[proc_macro_attribute]
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    internal::runner_main(args, item).into()
}

/// Derive `TypeIter` for a type.
#[proc_macro_derive(TypeIter, attributes(iter))]
pub fn type_iter(item: TokenStream) -> TokenStream {
    internal::type_iter(item).into()
}
