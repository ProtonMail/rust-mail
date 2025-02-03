use cruet::Inflector;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote};

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
/// enum ContactListResult { Ok(Vec<Contact>), Error(ContactError) }
/// fn contact_list() -> ContactListResult {
///     ContactListResult::from(Ok(vec![]))
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
pub fn export_result(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::Item);
    match input {
        syn::Item::Fn(func) => {
            let tokens = export_single_function(func);
            let new_func = tokens.new_func;
            if let Some(new_enum) = tokens.new_enum {
                TokenStream::from(quote! {
                    #new_enum

                    #[uniffi::export]
                    #new_func
                })
            } else {
                TokenStream::from(quote! {
                    #[uniffi::export]
                    #new_func
                })
            }
        }
        syn::Item::Impl(impl_block) => {
            let mut new_impl_items = Vec::new();
            let mut new_enums = Vec::new();
            let impl_type = if let syn::Type::Path(ref path) = impl_block.self_ty.as_ref() {
                path.path.segments.last().map(|s| s.ident.to_string())
            } else {
                None
            };

            for item in impl_block.items.iter().cloned() {
                if let syn::ImplItem::Fn(method) = item {
                    let tokens = export_single_method(impl_type.as_ref(), method);
                    let new_func = tokens.new_func;

                    new_impl_items.push(parse_quote!(#new_func));

                    if let Some(new_enum) = tokens.new_enum {
                        new_enums.push(new_enum);
                    }
                } else {
                    new_impl_items.push(item);
                }
            }

            let mut new_impl_block = impl_block.clone();
            new_impl_block.items = new_impl_items;

            TokenStream::from(quote! {
                #(#new_enums)*

                #[uniffi::export]
                #new_impl_block
            })
        }
        _ => TokenStream::from(
            quote! { compile_error!("Only functions or impl blocks are supported"); },
        ),
    }
}

struct GeneratedTokens {
    new_func: proc_macro2::TokenStream,
    new_enum: Option<proc_macro2::TokenStream>,
}

impl From<proc_macro2::TokenStream> for GeneratedTokens {
    fn from(tokens: proc_macro2::TokenStream) -> Self {
        GeneratedTokens {
            new_func: tokens,
            new_enum: None,
        }
    }
}

/// Export a single standalone function
///
fn export_single_function(mut input: syn::ItemFn) -> GeneratedTokens {
    let fn_name = &input.sig.ident;

    // Extract the Ok and Error types from the Result
    let (ok_type, err_type) = match extract_return_type(input.sig.output.clone()) {
        Some(Ok((ok_type, err_type))) => (ok_type, err_type),
        Some(Err(err)) => {
            return GeneratedTokens::from(quote! { #err #input });
        }
        _ => {
            // If the function does not return a Result, return the original function
            return GeneratedTokens::from(quote! { #input });
        }
    };

    // Create a new enum name
    let enum_name = syn::Ident::new(
        &format!("{fn_name}_result").to_pascal_case(),
        proc_macro2::Span::call_site(),
    );

    // Create new enum type with `From` implementation
    let new_enum = create_enum_type(&enum_name, &ok_type, &err_type);

    // Modify the return to use new enum type
    input.sig.output = syn::ReturnType::Type(
        syn::token::RArrow::default(),
        Box::new(parse_quote!(#enum_name)),
    );

    // Extract the existing function body
    let original_body = input.block.clone();

    // Modify the code block to call Into on the Result
    input.block = Box::new(parse_quote!({
        #enum_name::from(#original_body)
    }));

    GeneratedTokens {
        new_func: quote! { #input },
        new_enum: Some(new_enum),
    }
}

/// Export a single method from an impl block
///
fn export_single_method(impl_type: Option<&String>, mut input: syn::ImplItemFn) -> GeneratedTokens {
    let fn_name = &input.sig.ident;

    // Extract the Ok and Error types from the Result
    let (ok_type, err_type) = match extract_return_type(input.sig.output.clone()) {
        Some(Ok((ok_type, err_type))) => (ok_type, err_type),
        Some(Err(err)) => {
            return GeneratedTokens::from(quote! { #err #input });
        }
        _ => {
            // If the function does not return a Result, return the original function
            return GeneratedTokens::from(quote! { #input });
        }
    };

    // Create a new enum name
    let enum_name = if let Some(impl_type) = impl_type {
        format!("{impl_type}_{fn_name}_result").to_pascal_case()
    } else {
        format!("{fn_name}_result").to_pascal_case()
    };
    let enum_name = syn::Ident::new(&enum_name, proc_macro2::Span::call_site());

    let new_enum = create_enum_type(&enum_name, &ok_type, &err_type);

    // Modify the return to use new enum type
    input.sig.output = syn::ReturnType::Type(
        syn::token::RArrow::default(),
        Box::new(parse_quote!(#enum_name)),
    );

    // Extract the existing function body
    let original_body = input.block.clone();

    // Modify the code block to call Into on the Result
    input.block = parse_quote!({
        #enum_name::from(#original_body)
    });

    GeneratedTokens {
        new_func: quote! { #input },
        new_enum: Some(new_enum),
    }
}

fn create_enum_type(
    enum_name: &syn::Ident,
    ok_type: &syn::Type,
    err_type: &syn::Type,
) -> proc_macro2::TokenStream {
    // Empty for tuples
    let (ok_variant, ok_match_arm) = match &ok_type {
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => (quote! {}, quote! {}),
        _ => (quote! { (#ok_type) }, quote! { (val) }),
    };

    quote! {
        #[derive(uniffi::Enum)]
        pub enum #enum_name {
            Ok #ok_variant,
            Error(#err_type),
        }

        #[automatically_derived]
        impl From<::std::result::Result<#ok_type, #err_type>> for #enum_name
        {
            fn from(value: ::std::result::Result<#ok_type, #err_type>) -> Self {
                match value {
                    Ok(val) => Self::Ok #ok_match_arm,
                    Err(error) => {
                        ::tracing::error!("{error:?}");
                        #enum_name::Error(error)
                    }
                }
            }
        }
    }
}

/// Extracts the Ok and Error types from the Result type
/// Supplied Return type must have exactly two generic arguments.
/// Otherwise it will not be considerted a Result type.
///
/// ### Panics
///
/// This function will panic if the supplied Return type does not have exactly two generic arguments.
/// or if Result is aliased to some other type.
///
fn extract_return_type(
    output: syn::ReturnType,
) -> Option<Result<(syn::Type, syn::Type), proc_macro2::TokenStream>> {
    if let syn::ReturnType::Type(_, output) = output {
        if let syn::Type::Path(ref type_path) = *output {
            let last = type_path.path.segments.last()?;
            if &last.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(ref args) = last.arguments {
                    let mut args_iter = args.args.iter();
                    let syn::GenericArgument::Type(ref ok_type) = args_iter.next()? else {
                        let error_msg =
                            "Result type must have exactly two generic arguments, found none";
                        let error = quote! { compile_error!(#error_msg) };

                        return Some(Err(error));
                    };
                    let syn::GenericArgument::Type(ref err_type) = args_iter.next()? else {
                        let error_msg =
                            "Result type must have exactly two generic arguments, found one";
                        let error = quote! { compile_error!(#error_msg) };

                        return Some(Err(error));
                    };

                    if args_iter.next().is_some() {
                        let error_msg =
                            "Result type must have exactly two generic arguments, found more than two";
                        let error = quote! { compile_error!(#error_msg) };

                        return Some(Err(error));
                    }

                    return Some(Ok((ok_type.clone(), err_type.clone())));
                }
            } else if last.ident.to_string().contains("Result") {
                let error_msg = "Custom result type must be named `Result` and have exactly two generic arguments";
                let error = quote! { compile_error!(#error_msg) };
                return Some(Err(error));
            }
        }
    }

    None
}
