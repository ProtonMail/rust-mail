use crate::internal::prelude::*;
use std::ops::{Deref, DerefMut};
use syn::punctuated::Punctuated;
use syn::token::Paren;

/// A type that can be parsed via a terminated list of items.
pub trait ParseAll: Sized {
    fn parse_all(input: ParseStream) -> Result<Self>;
}

/// An extension trait for a parse stream.
pub trait ParseStreamExt {
    fn parse_all<T: ParseAll>(&self) -> Result<T>;
}

impl ParseStreamExt for ParseStream<'_> {
    fn parse_all<T: ParseAll>(&self) -> Result<T> {
        T::parse_all(self)
    }
}

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

/// A type that can return a dummy ident.
pub trait Dummy {
    fn dummy(&self) -> Ident;
}

/// Identifiers become dummies by being prefixed with `__`.
impl Dummy for Ident {
    fn dummy(&self) -> Ident {
        quote::format_ident!("__{self}")
    }
}

/// A type that can be converted into a `Type`.
pub trait AsType {
    fn as_type(&self) -> Type;
}

/// An identifier simply re-parses as a type.
impl AsType for Ident {
    fn as_type(&self) -> Type {
        parse_quote!(#self)
    }
}

/// A type that can be converted to an `Expr`.
pub trait AsExpr {
    fn as_expr(&self) -> Expr;
}

/// An identifier simply re-parses as an expression.
impl AsExpr for Ident {
    fn as_expr(&self) -> Expr {
        parse_quote!(#self)
    }
}

/// A type representing a `Foo(...)` construct, for use in argument parsing.
pub struct SubArg<K, V> {
    #[allow(unused)]
    pub token: K,

    #[allow(unused)]
    pub paren: Paren,

    pub value: V,
}

#[allow(unused)]
impl<K, V> SubArg<K, V> {
    pub fn map<T>(self, f: impl FnOnce(V) -> T) -> T {
        f(self.value)
    }

    pub fn map_ref<T>(&self, f: impl FnOnce(&V) -> T) -> T {
        f(&self.value)
    }
}

impl<K: Parse, T: Parse, P: Parse> ParseAll for SubArg<K, Punctuated<T, P>> {
    fn parse_all(input: ParseStream) -> Result<Self> {
        let inner;

        Ok(Self {
            token: input.parse()?,
            paren: parenthesized!(inner in input),
            value: Punctuated::parse_terminated(&inner)?,
        })
    }
}

impl<K: Parse, V: Parse> Parse for SubArg<K, V> {
    fn parse(input: ParseStream) -> Result<Self> {
        let inner;

        Ok(Self {
            token: input.parse()?,
            paren: parenthesized!(inner in input),
            value: inner.parse()?,
        })
    }
}

/// Extension methods for `Generics`.
pub trait GenericsExt {
    fn impl_gens(&self) -> ImplGens;
    fn type_gens(&self) -> TypeGens;
    fn where_preds(&self) -> WherePreds;
}

impl GenericsExt for Generics {
    fn impl_gens(&self) -> ImplGens {
        let mut res = Vec::new();

        for param in self.lifetimes() {
            res.push(GenericParam::Lifetime(param.to_owned()));
        }

        for param in self.const_params() {
            res.push(GenericParam::Const(param.to_owned()));
        }

        for param in self.type_params() {
            res.push(GenericParam::Type(param.to_owned()));
        }

        ImplGens(res)
    }

    fn type_gens(&self) -> TypeGens {
        let mut res = Vec::new();

        for param in self.lifetimes() {
            res.push(GenericArgument::Lifetime(param.lifetime.clone()));
        }

        for param in self.const_params() {
            res.push(GenericArgument::Const(param.ident.as_expr()));
        }

        for param in self.type_params() {
            res.push(GenericArgument::Type(param.ident.as_type()));
        }

        TypeGens(res)
    }

    fn where_preds(&self) -> WherePreds {
        let mut res = Vec::new();

        if let Some(clause) = &self.where_clause {
            for pred in &clause.predicates {
                res.push(pred.to_owned());
            }
        }

        WherePreds(res)
    }
}

/// The generic parameters of an `impl` block
/// (i.e. `impl<...>`, where `...` are the generic parameters).
pub struct ImplGens(Vec<GenericParam>);

impl ToTokens for ImplGens {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let inner = &self.0;

        if !inner.is_empty() {
            tokens.extend(quote!(<#(#inner),*>));
        }
    }
}

impl Deref for ImplGens {
    type Target = Vec<GenericParam>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ImplGens {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The generic arguments of a type
/// (i.e. `Foo<...>`, where `...` are the generic arguments).
pub struct TypeGens(Vec<GenericArgument>);

impl ToTokens for TypeGens {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let inner = &self.0;

        if !inner.is_empty() {
            tokens.extend(quote!(<#(#inner),*>));
        }
    }
}

impl Deref for TypeGens {
    type Target = Vec<GenericArgument>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TypeGens {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The where predicates of a type
/// (i.e. `where ...`, where `...` are the where predicates).
pub struct WherePreds(Vec<WherePredicate>);

impl ToTokens for WherePreds {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let inner = &self.0;

        if !inner.is_empty() {
            tokens.extend(quote!(where #(#inner),*));
        }
    }
}

impl Deref for WherePreds {
    type Target = Vec<WherePredicate>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for WherePreds {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Extension methods for `Signature`.
pub trait SignatureExt {
    /// Get the arguments of the function signature as expressions.
    fn args(&self) -> Vec<Expr>;
}

impl SignatureExt for Signature {
    fn args(&self) -> Vec<Expr> {
        let mut rest = Vec::new();

        for input in &self.inputs {
            if let FnArg::Typed(PatType { pat, .. }) = input {
                rest.push(parse_quote!(#pat));
            }
        }

        rest
    }
}
