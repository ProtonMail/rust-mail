use quote::quote;
use std::rc::Rc;
use syn::punctuated::Punctuated;
use syn::*;

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

pub trait AsIdent {
    fn as_ident(&self) -> Option<Ident>;
}

impl<T: AsIdent> AsIdent for Option<T> {
    fn as_ident(&self) -> Option<Ident> {
        self.as_ref().and_then(AsIdent::as_ident)
    }
}

impl<T: AsIdent> AsIdent for Rc<T> {
    fn as_ident(&self) -> Option<Ident> {
        self.as_ref().as_ident()
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

pub trait AsCall {
    fn as_call(&self) -> Expr;
}

impl AsCall for Signature {
    fn as_call(&self) -> Expr {
        let name = &self.ident;

        let (path, args) = if self.is_method() {
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

pub trait SignatureExt {
    fn is_method(&self) -> bool;
    fn call_args(&self) -> Vec<Expr>;
    fn with_name(&self, ident: Ident) -> Self;
}

impl SignatureExt for Signature {
    fn is_method(&self) -> bool {
        matches!(self.inputs.first(), Some(FnArg::Receiver(_)))
    }

    fn call_args(&self) -> Vec<Expr> {
        self.inputs.iter().map(AsExpr::as_expr).collect()
    }

    fn with_name(&self, ident: Ident) -> Self {
        Self {
            ident,
            ..self.to_owned()
        }
    }
}

pub trait ResultTypeExt {
    fn get_variants(&self) -> Option<(Type, Type)>;
}

impl ResultTypeExt for ReturnType {
    fn get_variants(&self) -> Option<(Type, Type)> {
        let ReturnType::Type(_, ty) = self else {
            return None;
        };

        let Type::Path(TypePath { path, .. }) = ty.as_ref() else {
            return None;
        };

        let Some(PathSegment { arguments, .. }) = path.segments.last() else {
            return None;
        };

        let PathArguments::AngleBracketed(args) = arguments else {
            return None;
        };

        if let [t, e] = args.args.to_vec().as_slice() {
            Some((parse_quote!(#t), parse_quote!(#e)))
        } else {
            None
        }
    }
}

pub trait ToVec<T> {
    fn to_vec(&self) -> Vec<&T>;
}

impl<T, P> ToVec<T> for Punctuated<T, P> {
    fn to_vec(&self) -> Vec<&T> {
        self.iter().collect()
    }
}
