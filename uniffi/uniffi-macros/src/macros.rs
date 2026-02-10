use crate::prelude::*;

/// A type that can be rendered as a token stream.
pub trait Render {
    fn render(self) -> TokenStream;
}

/// Identity: token streams render as themselves.
impl Render for TokenStream {
    fn render(self) -> TokenStream {
        self
    }
}

/// Results render as their inner value or as an error.
impl<T: ToTokens> Render for Result<T> {
    fn render(self) -> TokenStream {
        match self {
            Ok(t) => t.into_token_stream(),
            Err(e) => e.into_compile_error(),
        }
    }
}

macro_rules! render {
    ($($args:ident,)* {$expr:expr}) => {{
        use $crate::macros::Render;

        $(
            let $args = match ::syn::parse($args) {
                Ok(args) => args,
                Err(e) => return e.to_compile_error().into(),
            };
        )*

        $expr.render().into()
    }};
}
