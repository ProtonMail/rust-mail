//! For easier composition we define the [`Delimited`] type here which is a `T`
//! followed by a optional delimiting entity `D`. This is used by the
//! [`DelimitedVec`] type to parse a list of entities separated by a delimiter.

#![allow(clippy::module_name_repetitions)]

#[allow(clippy::wildcard_imports)]
use crate::*;

/// This is used when one wants to parse a list of entities separated by delimiters. The
/// delimiter is optional and can be `None` eg. when the entity is the last in the
/// list. Usually the delimiter will be some simple punctuation token, but it is not limited
/// to that.
#[derive(Clone)]
pub struct Delimited<T, D> {
    /// The parsed value
    pub value: T,
    /// The optional delimiter
    pub delimiter: Option<D>,
}

impl<T: Parse, D: Parse> Parser for Delimited<T, D> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        Ok(Self {
            value: T::parser(tokens)?,
            delimiter: Option::<D>::parser(tokens)?,
        })
    }
}

impl<T: ToTokens, D: ToTokens> ToTokens for Delimited<T, D> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.value.to_tokens(tokens);
        self.delimiter.to_tokens(tokens);
    }
}

#[mutants::skip]
impl<T: std::fmt::Debug, D: std::fmt::Debug> std::fmt::Debug for Delimited<T, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct(&format!(
            "Delimited<{}, {}>",
            std::any::type_name::<T>(),
            std::any::type_name::<D>()
        ))
        .field("value", &self.value)
        .field("delimiter", &self.delimiter)
        .finish()
    }
}

/// `T` followed by an optional `,`
pub type CommaDelimited<T> = Delimited<T, Comma>;
/// `T` followed by an optional `:`
pub type ColonDelimited<T> = Delimited<T, Colon>;
/// `T` followed by an optional `;`
pub type SemicolonDelimited<T> = Delimited<T, Semicolon>;
/// `T` followed by an optional `.`
pub type DotDelimited<T> = Delimited<T, Dot>;
/// `T` followed by an optional `::`
pub type PathSepDelimited<T> = Delimited<T, PathSep>;
