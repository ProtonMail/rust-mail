//! This module contains types for punctuation tokens. These are used to represent single and
//! multi character punctuation tokens. For single character punctuation tokens, there are
//! there are [`PunctAny`], [`PunctAlone`] and [`PunctJoint`] types.
#![allow(clippy::module_name_repetitions)]

pub use proc_macro2::Spacing;

use crate::{Error, Parser, Punct, Result, ToTokens, TokenIter, TokenStream, TokenTree};

/// A single character punctuation token with any kind of [`Spacing`],
#[derive(Default, Clone)]
pub struct PunctAny<const C: char>;

impl<const C: char> Parser for PunctAny<C> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        match tokens.next() {
            Some(TokenTree::Punct(punct)) if punct.as_char() == C => Ok(Self),
            _ => Error::unexpected_token(tokens),
        }
    }
}

impl<const C: char> ToTokens for PunctAny<C> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        Punct::new(C, Spacing::Alone).to_tokens(tokens);
    }
}

/// Convert a `PunctAny` object into a `TokenTree`.
impl<const C: char> From<PunctAny<C>> for TokenTree {
    fn from(_: PunctAny<C>) -> Self {
        TokenTree::Punct(Punct::new(C, Spacing::Alone))
    }
}

#[mutants::skip]
impl<const C: char> std::fmt::Debug for PunctAny<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PunctAny<{C:?}>")
    }
}

/// A single character punctuation token where the lexer joined it with the next `Punct` or a
/// single quote followed by a identifier (rust lifetime).
///
/// # Example
///
/// ```
/// # use unsynn::*;
/// // The TokenStream::from_str() keeps the ':::'
/// let mut token_iter = ":::".to_token_iter();
///
/// let colon = PunctJoint::<':'>::parse(&mut token_iter).unwrap();
/// let colon = PunctJoint::<':'>::parse(&mut token_iter).unwrap();
/// let colon = PunctAny::<':'>::parse(&mut token_iter).unwrap();
///
/// // Caveat: The quote! macro won't join ':::' together
/// // let mut token_iter = quote::quote! {:::}.into_iter();
/// //
/// // let colon = PunctJoint::<':'>::parse(&mut token_iter).unwrap();
/// // let colon = PunctAny::<':'>::parse(&mut token_iter).unwrap();
/// // let colon = PunctAny::<':'>::parse(&mut token_iter).unwrap();
/// ```
#[derive(Default, Clone)]
pub struct PunctJoint<const C: char>;

impl<const C: char> PunctJoint<C> {
    /// Create a new [`PunctJoint`] object.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Get the `char` value this object represents.
    #[must_use]
    pub const fn as_char(&self) -> char {
        C
    }
}

impl<const C: char> Parser for PunctJoint<C> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        match tokens.next() {
            Some(TokenTree::Punct(punct))
                if punct.spacing() == Spacing::Joint && punct.as_char() == C =>
            {
                Ok(Self)
            }
            _ => Error::unexpected_token(tokens),
        }
    }
}

impl<const C: char> ToTokens for PunctJoint<C> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        Punct::new(C, Spacing::Joint).to_tokens(tokens);
    }
}

#[mutants::skip]
impl<const C: char> std::fmt::Debug for PunctJoint<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PunctJoint<{C:?}>")
    }
}

/// Convert a [`PunctJoint`] object into a [`TokenTree`].
impl<const C: char> From<PunctJoint<C>> for TokenTree {
    fn from(_: PunctJoint<C>) -> Self {
        TokenTree::Punct(Punct::new(C, Spacing::Joint))
    }
}

#[test]
fn test_joint_punct_into_tt() {
    let mut token_iter = "+=".to_token_iter();
    let plus = PunctJoint::<'+'>::parser(&mut token_iter).unwrap();
    assert_eq!(plus.as_char(), '+');
    let _: TokenTree = plus.into();
}

/// A single character punctuation token which is not followed by another punctuation
/// character.
///
/// # Example
///
/// ```
/// # use unsynn::*;
/// let mut token_iter = ": :".to_token_iter();
///
/// let colon = PunctAlone::<':'>::parse(&mut token_iter).unwrap();
/// let colon = PunctAlone::<':'>::parse(&mut token_iter).unwrap();
/// ```
#[derive(Default, Clone)]
pub struct PunctAlone<const C: char>;

impl<const C: char> PunctAlone<C> {
    /// Create a new [`PunctAlone`] object.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Get the `char` value this object represents.
    #[must_use]
    pub const fn as_char(&self) -> char {
        C
    }
}

impl<const C: char> Parser for PunctAlone<C> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        match tokens.next() {
            Some(TokenTree::Punct(punct))
                if punct.spacing() == Spacing::Alone && punct.as_char() == C =>
            {
                Ok(Self)
            }
            _ => Error::unexpected_token(tokens),
        }
    }
}

impl<const C: char> ToTokens for PunctAlone<C> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        Punct::new(C, Spacing::Alone).to_tokens(tokens);
    }
}

#[mutants::skip]
impl<const C: char> std::fmt::Debug for PunctAlone<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PunctAlone<{C:?}>")
    }
}

/// Convert a [`PunctAlone`] object into a [`TokenTree`].
impl<const C: char> From<PunctAlone<C>> for TokenTree {
    fn from(_: PunctAlone<C>) -> Self {
        TokenTree::Punct(Punct::new(C, Spacing::Alone))
    }
}

#[test]
fn test_alone_punct_into_tt() {
    let mut token_iter = "+ +".to_token_iter();
    let plus = PunctAlone::<'+'>::parser(&mut token_iter).unwrap();
    assert_eq!(plus.as_char(), '+');
    let _: TokenTree = plus.into();
}
