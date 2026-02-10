use crate::internal::prelude::*;
use crate::internal::util::{Dummy, GenericsExt};
use syn::punctuated::Punctuated;

#[must_use]
pub fn derive_dyn<T: Into<TokenStream>>(args: T, item: T) -> TokenStream {
    let args = args.into();
    let item = item.into();

    render!(args, item, { expand(&args, &item) })
}

fn expand(args: &Args, item: &ItemTrait) -> TokenStream {
    let mut res = quote!(#item);

    for arg in &args.0 {
        match arg {
            Arg::Debug(_) => res.extend(expand_debug(item)),
        }
    }

    res
}

fn expand_debug(item: &ItemTrait) -> TokenStream {
    let ident = &item.ident;

    let mut igen = item.generics.impl_gens();
    let mut tgen = item.generics.type_gens();
    let mut wgen = item.generics.where_preds();

    for item in &item.items {
        if let TraitItem::Type(item) = item {
            let ident = &item.ident;
            let dummy = ident.dummy();
            let bounds = &item.bounds;

            igen.push(parse_quote!(#dummy));
            tgen.push(parse_quote!(#ident = #dummy));
            wgen.push(parse_quote!(#dummy: #bounds));
        }
    }

    quote! {
        const _: () = {
            impl #igen ::std::fmt::Debug for dyn #ident #tgen #wgen {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    f.debug_struct(::std::any::type_name::<Self>()).finish()
                }
            }
        };
    }
}

/// Arguments to the `derive_dyn` attribute.
struct Args(Punctuated<Arg, Token![,]>);

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self(Punctuated::parse_terminated(input)?))
    }
}

// The `Debug` custom keyword.
custom_keyword!(Debug);

/// A single argument to the `derive_dyn` attribute.
#[allow(unused)]
enum Arg {
    Debug(Debug),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        let token = input.lookahead1();

        if token.peek(Debug) {
            return Ok(Self::Debug(input.parse()?));
        }

        Err(token.error())
    }
}
