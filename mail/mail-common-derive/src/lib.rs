//! Derive macros for mail-common functionality.
//!
//! This crate provides procedural macros specific to mail functionality,
//! particularly for conversation and message handling.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::ParseStream;
use syn::{Data, DeriveInput, Field, Fields, Ident, parse_macro_input};

/// Automatically derive the `ScrollerEq` trait for a struct.
///
/// This macro generates an implementation of the `ScrollerEq` trait that compares
/// all fields for equality except those marked with `#[scroller_eq(skip)]`.
///
/// # Attributes
///
/// - `#[scroller_eq(skip)]`: Skip this field from equality comparison
///
/// # Example
///
/// ```rust
/// use mail_common_derive::ScrollerEq;
///
/// #[derive(ScrollerEq)]
/// struct Conversation {
///     id: u64,
///     subject: String,
///     #[scroller_eq(skip)]
///     last_updated: u64,  // This field won't be compared
///     #[scroller_eq(skip)]
///     unread_count: u32,  // This field won't be compared
/// }
///
/// let conv1 = Conversation {
///     id: 1,
///     subject: "Hello".to_string(),
///     last_updated: 100,
///     unread_count: 5,
/// };
///
/// let conv2 = Conversation {
///     id: 1,
///     subject: "Hello".to_string(),
///     last_updated: 200,  // Different timestamp
///     unread_count: 3,    // Different count
/// };
///
/// assert!(conv1.scroller_eq(&conv2)); // Returns true despite different skipped fields
/// ```
#[proc_macro_derive(ScrollerEq, attributes(scroller_eq))]
pub fn scroller_eq_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Extract fields that should be compared (those without #[scroller_eq(skip)])
    let fields = extract_fields(&input, "ScrollerEq");
    let comparison_fields = extract_scroller_eq_fields(&fields);

    // Generate the comparison implementation
    let field_comparisons = comparison_fields.iter().map(|field_ident| {
        quote! {
            self.#field_ident == other.#field_ident
        }
    });

    let comparison_impl = if comparison_fields.is_empty() {
        // If no fields to compare, always return true
        quote! { true }
    } else {
        // Chain all field comparisons with &&
        quote! {
            #(#field_comparisons)&&*
        }
    };

    (quote! {
        impl #impl_generics crate::traits::ScrollerEq for #name #ty_generics #where_clause {
            fn scroller_eq(&self, other: &Self) -> bool {
                #comparison_impl
            }
        }
    })
    .into()
}

/// Extract the fields of a struct.
///
/// This function extracts the fields of a struct, ensuring that the struct is
/// indeed a struct with named fields.
fn extract_fields<'a>(input: &'a DeriveInput, macro_name: &'a str) -> Vec<&'a Field> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            fields.named.iter().collect::<Vec<_>>()
        } else {
            panic!("{macro_name} can only be derived for structs with named fields")
        }
    } else {
        panic!("{macro_name} can only be derived for structs")
    }
}

/// Extract fields that should be included in ScrollerEq comparison.
///
/// This function filters out fields that have the `#[scroller_eq(skip)]` attribute.
fn extract_scroller_eq_fields<'a>(fields: &'a [&'a Field]) -> Vec<&'a Ident> {
    fields
        .iter()
        .filter_map(|field| {
            // Check if field has #[scroller_eq(skip)] attribute
            let should_skip = field.attrs.iter().any(|attr| {
                if attr.path().is_ident("scroller_eq") {
                    // Parse the attribute arguments to see if it contains "skip"
                    if let Ok(args) = attr.parse_args_with(|input: ParseStream| {
                        let ident: Ident = input.parse()?;
                        Ok(ident)
                    }) {
                        args == "skip"
                    } else {
                        false
                    }
                } else {
                    false
                }
            });

            if should_skip {
                None
            } else {
                field.ident.as_ref()
            }
        })
        .collect()
}
