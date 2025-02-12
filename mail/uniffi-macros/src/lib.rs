use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Implements the `#[export_result]` attribute macro.
mod export_result;

/// Prelude for the crate.
mod prelude;

/// Helper traits for working with `syn` types.
mod traits;

/// This `proc_macro` rewrites the function which return Result<T, E> to return non genric enum type.
/// If used on a function that does not return Result, it will return the original function.
///
/// This is usefull for `#[uniffi::export]` to not throw panics on client side.
/// This macro can be used interchangable with `#[uniffi::export]` attribute as it expands to include it.
///
/// New enum type uses function name as a prefix and `Result` as a suffix.
/// It currently supports only functions and impl blocks. For impl blocks it will rewrite all the methods returning Result type.
///
/// ### Example
///
/// ```ignore
/// #[proton_uniffi_macros::export_result]
/// fn contact_list() -> Result<Vec<Contact>, ContactError> {
///     Ok(vec![])
/// }
/// ```
///
/// will be rewritten as
///
/// ```ignore
/// enum ContactListResult {
///     Ok(Vec<Contact>),
///     Error(ContactError),
/// }
///
/// #[uniffi::export]
/// fn contact_list() -> ContactListResult {
///     ContactListResult::from(__contact_list())
/// }
///
/// fn __contact_list() -> Result<Vec<Contact>, ContactError> {
///     Ok(vec![])
/// }
/// ```
///
/// ### Panics
///
/// This macro will panic if the `TokenStream` is not a function or impl block.
/// Also if the function returns a Result type, it need to have exactly two generic arguments.
///
/// ### Limitations
///
/// Return type must be exactly `Result<T, E>` where `T` and `E` are generic types.
/// There is no support for type aliases or other generic types.
///
/// Take into considaration that if there is a function with the same name as
/// the method in impl block and both are using this macro, there will be a compile error
/// as the enum type will be duplicated.
///
/// The same applies for the `*` imports & exports - this may be addressed in the future.
/// But for now use unique and descripitive names for exported functions and methods.
///
#[proc_macro_attribute]
pub fn export_result(_: TokenStream, input: TokenStream) -> TokenStream {
    export_result::expand(parse_macro_input!(input as syn::Item)).into()
}
