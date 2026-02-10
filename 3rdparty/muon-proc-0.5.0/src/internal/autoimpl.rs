use crate::internal::prelude::*;
use crate::internal::util::{GenericsExt, ParseStreamExt, SignatureExt, SubArg};
use syn::punctuated::Punctuated;

#[must_use]
pub fn autoimpl<T: Into<TokenStream>>(args: T, item: T) -> TokenStream {
    let args = args.into();
    let item = item.into();

    render!(args, item, { expand(&args, &item) })
}

fn expand(args: &Args, item: &ItemTrait) -> TokenStream {
    let mut tokens = quote!(#item);
    let mut r#for = Vec::new();
    let mut r#where = Vec::new();

    for arg in &args.0 {
        match arg {
            Arg::For(arg) => r#for.extend(&arg.value),
            Arg::Where(arg) => r#where.extend(&arg.value),
        }
    }

    if r#for.is_empty() {
        tokens.extend(expand_where(item, &r#where));
    } else {
        for this in &r#for {
            tokens.extend(expand_for(item, this));
        }
    }

    tokens
}

fn expand_where(item: &ItemTrait, r#where: &[&TypeParam]) -> TokenStream {
    let ident = &item.ident;
    let dummy = format_ident!("This");

    let mut igen = item.generics.impl_gens();
    let tgen = item.generics.type_gens();
    let mut wgen = item.generics.where_preds();

    igen.push(parse_quote!(#dummy));
    wgen.extend(item.supertraits.iter().map(|b| parse_quote!(#dummy: #b)));
    wgen.extend(r#where.iter().map(|b| parse_quote!(#b)));

    quote! {
        const _: () = {
            impl #igen #ident #tgen for #dummy #wgen {}
        };
    }
}

fn expand_for(item: &ItemTrait, this: &Type) -> TokenStream {
    let ident = &item.ident;

    let igen = item.generics.impl_gens();
    let tgen = item.generics.type_gens();
    let wgen = item.generics.where_preds();

    let mut items = Vec::new();

    for item in &item.items {
        if let TraitItem::Fn(TraitItemFn { sig, .. }) = item {
            let name = &sig.ident;
            let args = sig.args();
            let this = sig.receiver().map(deref);

            items.push(quote!(#sig { #this.#name(#(#args),*) }));
        }
    }

    quote! {
        const _: () = {
            impl #igen #ident #tgen for #this #wgen {
                #(#items)*
            }
        };
    }
}

/// Arguments to the `derive_dyn` attribute.
pub struct Args(Punctuated<Arg, Token![,]>);

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self(Punctuated::parse_terminated(input)?))
    }
}

/// A single argument option supported by the `http` attribute.
enum Arg {
    For(SubArg<Token![for], Punctuated<Type, Token![,]>>),
    Where(SubArg<Token![where], Punctuated<TypeParam, Token![,]>>),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(Token![for]) {
            return Ok(Self::For(input.parse_all()?));
        }

        if lookahead.peek(Token![where]) {
            return Ok(Self::Where(input.parse_all()?));
        }

        Err(lookahead.error())
    }
}

fn deref(r: &Receiver) -> TokenStream {
    let this = r.self_token;

    if r.mutability.is_some() {
        quote! {{
            use ::std::ops::DerefMut;

            #this.deref_mut()
        }}
    } else {
        quote! {{
            use ::std::ops::Deref;

            #this.deref()
        }}
    }
}
