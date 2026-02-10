use proc_macro::TokenStream;

#[macro_use]
mod macros;

/// Implements the `#[uniffi_export]` attribute macro.
mod uniffi_export;

/// Prelude for the crate.
mod prelude;

/// Helper traits for working with `syn` types.
mod traits;

/// This `proc_macro` rewrites the function which return Result<T, E> to return non genric enum type.
/// If used on a function that does not return Result, or which does not have exactly two generic arguments,
/// it will return the original function.
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
/// ### Limitations
///
/// Return type must be exactly `Result<T, E>` where `T` and `E` are generic types.
/// There is no support for type aliases or other generic types; the macro will be a
/// no-op on such functions.
///
/// Take into considaration that if there is a function with the same name as
/// the method in impl block and both are using this macro, there will be a compile error
/// as the enum type will be duplicated.
///
/// The same applies for the `*` imports & exports - this may be addressed in the future.
/// But for now use unique and descripitive names for exported functions and methods.
///
#[proc_macro_attribute]
pub fn uniffi_export(_: TokenStream, item: TokenStream) -> TokenStream {
    render!(item, { uniffi_export::expand(item) })
}
