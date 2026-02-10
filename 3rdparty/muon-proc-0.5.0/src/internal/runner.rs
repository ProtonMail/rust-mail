use crate::internal::prelude::*;
use crate::internal::util::SubArg;
use syn::punctuated::Punctuated;

#[must_use]
pub fn runner_test<T: Into<TokenStream>>(args: T, item: T) -> TokenStream {
    let args = args.into();
    let item = item.into();

    render!(args, item, { expand_test(&args, &item) })
}

#[must_use]
pub fn runner_main<T: Into<TokenStream>>(args: T, item: T) -> TokenStream {
    let args = args.into();
    let item = item.into();

    render!(args, item, { expand_main(&args, &item) })
}

/// Expands the `#[test]` attribute.
fn expand_test(args: &Args, item: &ItemFn) -> TokenStream {
    expand(args, item, false)
}

/// Expands the `#[main]` attribute.
fn expand_main(args: &Args, item: &ItemFn) -> TokenStream {
    expand(args, item, true)
}

fn expand(args: &Args, item: &ItemFn, main: bool) -> TokenStream {
    // Get the top-level item fields.
    let attrs = &item.attrs;
    let vis = &item.vis;
    let sig = &item.sig;
    let block = &item.block;

    // Get the relevant signature fields.
    let ident = &sig.ident;
    let inputs = &sig.inputs;
    let output = &sig.output;

    // Build the arguments.
    let args = args.0.iter().map(|arg| match arg {
        Arg::Scheme(arg) => arg.map_ref(|v| quote!(scheme(#v))),
        Arg::User(arg) => arg.map_ref(|UserArg(n, _, p)| quote!(user(#n, #p))),
    });

    // If not main, build the additional test attribute.
    let test = if !main {
        quote! { #[::core::prelude::v1::test] }
    } else {
        quote! {}
    };

    // Build the new function.
    quote! {
        #(#attrs)*
        #test #vis fn #ident() #output {
            use ::muon::test::runner::Args;
            use ::muon::test::runner::run;

            run(Args::default()#(.#args)*, |#inputs| async move #block)
        }
    }
}

struct Args(Punctuated<Arg, Token![,]>);

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self(Punctuated::parse_terminated(input)?))
    }
}

custom_keyword!(scheme);
custom_keyword!(user);
custom_keyword!(pass);

enum Arg {
    Scheme(SubArg<scheme, Expr>),
    User(SubArg<user, UserArg>),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        let token = input.lookahead1();

        if token.peek(scheme) {
            return Ok(Self::Scheme(input.parse()?));
        }

        if token.peek(user) {
            return Ok(Self::User(input.parse()?));
        }

        Err(token.error())
    }
}

#[allow(unused)]
struct UserArg(Expr, Token![,], Expr);

impl Parse for UserArg {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self(input.parse()?, input.parse()?, input.parse()?))
    }
}
