use crate::prelude::*;
use syn::{fold::Fold, punctuated::Punctuated};

/// Converts a type into a `syn::Expr`.
/// Used for converting function arguments into expressions for generating function calls.
pub trait AsExpr {
    fn as_expr(&self) -> Expr;
}

impl AsExpr for FnArg {
    fn as_expr(&self) -> Expr {
        match self {
            FnArg::Receiver(arg) => arg.as_expr(),
            FnArg::Typed(arg) => arg.as_expr(),
        }
    }
}

impl AsExpr for Receiver {
    fn as_expr(&self) -> Expr {
        let Self { self_token, .. } = self;

        parse_quote!(#self_token)
    }
}

impl AsExpr for PatType {
    fn as_expr(&self) -> Expr {
        let Self { pat, .. } = self;

        parse_quote!(#pat)
    }
}

/// Attempts to convert a type into a `syn::Ident`.
/// Used for type name resolution and identifier extraction from various syn types.
pub trait AsIdent {
    fn as_ident(&self) -> Option<Ident>;
}

impl<T: AsIdent> AsIdent for &T {
    fn as_ident(&self) -> Option<Ident> {
        (*self).as_ident()
    }
}

impl<T: AsIdent> AsIdent for Option<T> {
    fn as_ident(&self) -> Option<Ident> {
        self.as_ref().and_then(AsIdent::as_ident)
    }
}

impl AsIdent for Type {
    fn as_ident(&self) -> Option<Ident> {
        if let Type::Path(t) = self {
            t.as_ident()
        } else {
            None
        }
    }
}

impl AsIdent for TypePath {
    fn as_ident(&self) -> Option<Ident> {
        self.path.get_ident().cloned()
    }
}

/// Converts a type into `syn::Fields`.
/// Used for generating enum variant fields from types, particularly in Result type handling.
pub trait AsFields {
    fn as_fields(&self) -> Fields;
}

impl AsFields for Type {
    fn as_fields(&self) -> Fields {
        if let Type::Tuple(t) = self {
            t.as_fields()
        } else {
            Fields::Unnamed(parse_quote!((#self)))
        }
    }
}

impl AsFields for TypeTuple {
    fn as_fields(&self) -> Fields {
        if self.elems.is_empty() {
            Fields::Unit
        } else {
            Fields::Unnamed(parse_quote!(#self))
        }
    }
}

/// Generates match expression right-hand-side for a type.
/// Used in Result enum conversion implementations for pattern matching.
pub trait AsMatch {
    fn as_match_rhs(&self, v: &Ident) -> Option<Expr>;
}

impl AsMatch for Type {
    fn as_match_rhs(&self, v: &Ident) -> Option<Expr> {
        if let Type::Tuple(t) = self {
            t.as_match_rhs(v)
        } else {
            Some(parse_quote!((#v)))
        }
    }
}

impl AsMatch for TypeTuple {
    fn as_match_rhs(&self, v: &Ident) -> Option<Expr> {
        if self.elems.is_empty() {
            None
        } else {
            Some(parse_quote!((#v)))
        }
    }
}

/// Converts a type into a function call expression.
/// Used for generating function call syntax in code generation.
pub trait AsCall {
    fn as_call(&self, this: Option<&Type>) -> Expr;
}

impl AsCall for Signature {
    fn as_call(&self, this: Option<&Type>) -> Expr {
        let name = &self.ident;

        let (path, args) = if this.is_some() {
            (quote!(Self::#name), self.call_args())
        } else {
            (quote!(#name), self.call_args())
        };

        if self.asyncness.is_some() {
            parse_quote!(#path(#(#args),*).await)
        } else {
            parse_quote!(#path(#(#args),*))
        }
    }
}

/// Converts a type into the `Ok` and `Error` variants of a Result type.
pub trait AsVariants {
    fn as_variants(&self) -> Option<(Type, Type)>;
}

impl AsVariants for ReturnType {
    fn as_variants(&self) -> Option<(Type, Type)> {
        if let ReturnType::Type(_, t) = self {
            t.as_variants()
        } else {
            None
        }
    }
}

impl AsVariants for Type {
    fn as_variants(&self) -> Option<(Type, Type)> {
        if let Type::Path(TypePath { path, .. }) = self {
            path.as_variants()
        } else {
            None
        }
    }
}

impl AsVariants for Path {
    fn as_variants(&self) -> Option<(Type, Type)> {
        if let Some(PathSegment { arguments, .. }) = self.segments.last() {
            arguments.as_variants()
        } else {
            None
        }
    }
}

impl AsVariants for PathArguments {
    fn as_variants(&self) -> Option<(Type, Type)> {
        let PathArguments::AngleBracketed(args) = self else {
            return None;
        };

        if let [t, e] = args.args.to_vec().as_slice() {
            return Some((parse_quote!(#t), parse_quote!(#e)));
        }

        None
    }
}

/// Extension trait for `syn::Ident`.
pub trait IdentExt {
    fn private(&self) -> Ident;
}

impl IdentExt for Ident {
    fn private(&self) -> Ident {
        format_ident!("__{self}")
    }
}

/// Extension trait for `Path`.
pub trait PathExt {
    fn is_self(&self) -> bool;
}

impl PathExt for Path {
    fn is_self(&self) -> bool {
        self.is_ident("Self")
    }
}

/// Extension trait for `Type`.
pub trait TypeExt {
    fn fold_self(&self, this: &Type) -> Type;
}

impl TypeExt for Type {
    fn fold_self(&self, this: &Type) -> Type {
        struct Folder<'a>(&'a Type);

        impl Fold for Folder<'_> {
            fn fold_type(&mut self, ty: Type) -> Type {
                let Type::Path(TypePath { path, .. }) = &ty else {
                    return fold::fold_type(self, ty);
                };

                if !path.is_self() {
                    return fold::fold_type(self, ty);
                }

                self.0.to_owned()
            }
        }

        Fold::fold_type(&mut Folder(this), self.to_owned())
    }
}

/// Extension trait for `syn::Signature`.
pub trait SignatureExt {
    fn call_args(&self) -> Vec<Expr>;
}

impl SignatureExt for Signature {
    fn call_args(&self) -> Vec<Expr> {
        self.inputs.iter().map(AsExpr::as_expr).collect()
    }
}

/// Extension trait for `Vec<Attribute>`.
pub trait AttributesExt {
    fn pop_arg_for<T>(&mut self, name: &str) -> Option<T>
    where
        T: Parse;
}

impl AttributesExt for Vec<Attribute> {
    fn pop_arg_for<T>(&mut self, name: &str) -> Option<T>
    where
        T: Parse,
    {
        for (idx, att) in self.iter().enumerate() {
            if att.path().is_ident(name) {
                return self.remove(idx).parse_args().ok();
            }
        }

        None
    }
}

/// Extension trait for `syn::Punctuated`.
pub trait PunctuatedExt<T> {
    fn to_vec(&self) -> Vec<&T>;
}

impl<T, P> PunctuatedExt<T> for Punctuated<T, P> {
    fn to_vec(&self) -> Vec<&T> {
        self.iter().collect()
    }
}
