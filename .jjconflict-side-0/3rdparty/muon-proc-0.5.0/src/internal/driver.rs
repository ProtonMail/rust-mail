use crate::internal::prelude::*;

pub fn driver<T: Into<TokenStream>>(args: T, item: T) -> TokenStream {
    let args = args.into();
    let item = item.into();

    render!(args, item, { expand(&args, &item) })
}

fn expand(args: &Args, item: &ItemFn) -> TokenStream {
    let attrs = &item.attrs;
    let vis = &item.vis;
    let sig = &item.sig;
    let block = &item.block;
    let driver = &args.0;

    quote! {
        #(#attrs)*
        #vis #sig {
            use ::futures::prelude::*;

            let this = async { #block };
            let with = async { (&#driver).await };

            ::futures::pin_mut! {
                this,
                with,
            }

            ::futures::select! {
                res = this.fuse() => res,
                ()  = with.fuse() => unreachable!(),
            }
        }
    }
}

struct Args(Expr);

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self(input.parse()?))
    }
}
