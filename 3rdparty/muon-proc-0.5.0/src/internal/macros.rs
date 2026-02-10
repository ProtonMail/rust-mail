macro_rules! render {
    ($($args:ident,)* {$expr:expr}) => {{
        use $crate::internal::util::Render;

        $(
            let $args = match ::syn::parse2($args) {
                Ok(args) => args,
                Err(e) => return e.to_compile_error().into(),
            };
        )*

        $expr.render().into()
    }};
}

pub(crate) use render;
