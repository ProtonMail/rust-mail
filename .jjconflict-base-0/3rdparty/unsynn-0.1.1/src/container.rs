//! This module provides parsers for types that contain possibly multiple values. This
//! includes stdlib types like [`Option`], [`Vec`], [`Box`], [`Rc`], [`RefCell`] and types
//! for delimited and repeated values with numbered repeats.

use crate::{
    Colon, Comma, Delimited, Dot, Error, Nothing, Parse, Parser, PathSep, Result, Semicolon,
    ToTokens, TokenIter, TokenStream,
};

use std::{cell::RefCell, rc::Rc};

/// Zero or One of T.
impl<T: Parse> Parser for Option<T> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        match T::parse(tokens) {
            Ok(value) => Ok(Some(value)),
            Err(_) => Ok(None),
        }
    }
}

impl<T: ToTokens> ToTokens for Option<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if let Some(t) = self.as_ref() {
            t.to_tokens(tokens);
        }
    }
}

/// Any number of T
impl<T: Parse> Parser for Vec<T> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        let mut output = Vec::new();
        while let Ok(value) = T::parse(tokens) {
            output.push(value);
        }
        Ok(output)
    }
}

impl<T: ToTokens> ToTokens for Vec<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.iter().for_each(|value| value.to_tokens(tokens));
    }
}

/// A trait for parsing a repeating `T` with a minimum and maximum limit.
/// Sometimes the number of elements to be parsed is determined at runtime eg. a number of
/// header items needs a matching number of values.
///
/// # Example
///
/// Parse at table with a number of headers followed by values.
///
/// ```
/// # use unsynn::*;
/// let mut token_iter = "
///     foo:       bar:
///     foo_value  bar_value
/// ".to_token_iter();
///
/// let headers = Vec::<Cons<Ident,Colon>>::parse(&mut token_iter).unwrap();
/// let values = Vec::<Ident>::parse_exactly(&mut token_iter, headers.len()).unwrap();
/// ```
#[allow(clippy::missing_errors_doc)]
pub trait RangedRepeats: Sized {
    /// Parse at least `min` and up to `max` (inclusive) elements.
    fn parse_repeats(tokens: &mut TokenIter, min: usize, max: usize) -> Result<Self>;

    /// Parse any number of elements.
    fn parse_any(tokens: &mut TokenIter) -> Result<Self> {
        Self::parse_repeats(tokens, 0, usize::MAX)
    }

    /// Parse at least one element.
    fn parse_many(tokens: &mut TokenIter) -> Result<Self> {
        Self::parse_repeats(tokens, 1, usize::MAX)
    }

    /// Parse zero or one element.
    fn parse_optional(tokens: &mut TokenIter) -> Result<Self> {
        Self::parse_repeats(tokens, 0, 1)
    }

    /// Parse exactly `n` elements.
    fn parse_exactly(tokens: &mut TokenIter, n: usize) -> Result<Self> {
        Self::parse_repeats(tokens, n, n)
    }

    /// Parse at most `n` elements.
    fn parse_at_most(tokens: &mut TokenIter, n: usize) -> Result<Self> {
        Self::parse_repeats(tokens, 0, n)
    }

    /// Parse at least `n` elements.
    fn parse_at_least(tokens: &mut TokenIter, n: usize) -> Result<Self> {
        Self::parse_repeats(tokens, n, usize::MAX)
    }
}

impl<T: Parse> RangedRepeats for Vec<T> {
    fn parse_repeats(tokens: &mut TokenIter, min: usize, max: usize) -> Result<Self> {
        let mut output = Vec::with_capacity(min);
        for _ in 0..max {
            if let Ok(value) = T::parse(tokens) {
                output.push(value);
            } else {
                break;
            }
        }

        if output.len() >= min {
            Ok(output)
        } else {
            Error::other(
                tokens,
                format!("less than {} elements, got {}", min, output.len()),
            )
        }
    }
}

/// Box a parseable entity. In a enum it may happen that most variants are rather small while
/// few variants are large. In this case it may be beneficial to box the large variants to
/// keep the enum lean. `Box` or `Rc` are required for parsing recursive grammars.
impl<T: Parse> Parser for Box<T> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        Ok(Box::new(T::parser(tokens)?))
    }
}

impl<T: ToTokens> ToTokens for Box<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.as_ref().to_tokens(tokens);
    }
}

/// Rc a parseable entity. Just because we can. Sometimes when a value is shared between
/// multiple entities it may be beneficial to use Rc. `Box` or `Rc` are required for parsing recursive grammars.
impl<T: Parse> Parser for Rc<T> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        Ok(Rc::new(T::parser(tokens)?))
    }
}

impl<T: ToTokens> ToTokens for Rc<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.as_ref().to_tokens(tokens);
    }
}

/// Put any parseable entity in a `RefCell`. In case one wants to mutate the a parse tree on the
/// fly.
impl<T: Parse> Parser for RefCell<T> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        Ok(RefCell::new(T::parser(tokens)?))
    }
}

impl<T: ToTokens> ToTokens for RefCell<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.borrow().to_tokens(tokens);
    }
}

/// A `Vec<T>` that is filled up to the first appearance of an terminating `S`.  This `S` may
/// be a subset of `T`, thus parsing become lazy.  This is the same as
/// `Cons<Vec<Cons<Except<S>,T>>,S>` but more convenient and efficient.
///
/// # Example
///
/// Parse anything until a `;`.
///
/// ```
/// # use unsynn::*;
/// let mut token_iter = "foo bar ; baz ;".to_token_iter();
///
/// type Example = LazyVec<TokenTree, Semicolon>;
///
/// let _example = Example::parse(&mut token_iter).unwrap();
/// let _example = Example::parse(&mut token_iter).unwrap();
/// ```
#[derive(Clone)]
pub struct LazyVec<T, S> {
    /// The vector of repeating `T`
    pub vec: Vec<T>,
    /// The terminating `S`
    pub terminator: S,
}

impl<T: Parse, S: Parse> Parser for LazyVec<T, S> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        let mut vec = Vec::new();

        loop {
            if let Ok(terminator) = S::parse(tokens) {
                return Ok(Self { vec, terminator });
            }

            vec.push(T::parse(tokens)?);
        }
    }
}

impl<T: ToTokens, S: ToTokens> ToTokens for LazyVec<T, S> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.vec.iter().for_each(|value| value.to_tokens(tokens));
        self.terminator.to_tokens(tokens);
    }
}

impl<T: Parse, S: Parse> RangedRepeats for LazyVec<T, S> {
    fn parse_repeats(tokens: &mut TokenIter, min: usize, max: usize) -> Result<Self> {
        let mut vec = Vec::with_capacity(min);
        for _ in 0..max {
            if let Ok(terminator) = S::parse(tokens) {
                return if vec.len() >= min {
                    Ok(Self { vec, terminator })
                } else {
                    Error::other(
                        tokens,
                        format!("less than {} elements, got {}", min, vec.len()),
                    )
                };
            }
            vec.push(T::parse(tokens)?);
        }
        Error::other(tokens, format!("more than {max} elements"))
    }
}

impl<T, S> IntoIterator for LazyVec<T, S> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.vec.into_iter()
    }
}

#[mutants::skip]
impl<T: std::fmt::Debug, S: std::fmt::Debug> std::fmt::Debug for LazyVec<T, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!(
            "LazyVec<{}, {}>",
            std::any::type_name::<T>(),
            std::any::type_name::<S>()
        ))
        .field("vec", &self.vec)
        .field("terminator", &self.terminator)
        .finish()
    }
}

/// Since the delimiter in [`Delimited<T,D>`] is optional a [`Vec<Delimited<T,D>>`] would parse
/// consecutive values even without delimiters. [`DelimitedVec<T,D>`] will stop parsing after
/// the first value without a delimiter.
#[derive(Clone)]
pub struct DelimitedVec<T, D>(pub Vec<Delimited<T, D>>);

impl<T: Parse, D: Parse> Parser for DelimitedVec<T, D> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        let mut output = Vec::new();
        while let Ok(delimited) = Delimited::<T, D>::parse(tokens) {
            let done = delimited.delimiter.is_none();
            output.push(delimited);
            if done {
                break;
            }
        }
        Ok(Self(output))
    }
}

/// Converts a [`DelimitedVec<T, D>`] into a [`Vec<T>`].
/// This loses all delimiters, which may have been stateful (`Either` or other enums).
impl<T, D> From<DelimitedVec<T, D>> for Vec<T> {
    fn from(delimited_vec: DelimitedVec<T, D>) -> Self {
        delimited_vec
            .0
            .into_iter()
            .map(|delimited| delimited.value)
            .collect()
    }
}

impl<T: ToTokens, D: ToTokens> ToTokens for DelimitedVec<T, D> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.iter().for_each(|value| value.to_tokens(tokens));
    }
}

impl<T: Parse, D: Parse> RangedRepeats for DelimitedVec<T, D> {
    fn parse_repeats(tokens: &mut TokenIter, min: usize, max: usize) -> Result<Self> {
        let mut output = Vec::with_capacity(min);
        for _ in 0..max {
            if let Ok(delimited) = Delimited::<T, D>::parse(tokens) {
                let done = delimited.delimiter.is_none();
                output.push(delimited);
                if done {
                    break;
                }
            } else {
                break;
            }
        }

        if output.len() >= min {
            Ok(Self(output))
        } else {
            Error::other(
                tokens,
                format!("less than {} elements, got {}", min, output.len()),
            )
        }
    }
}

impl<T, D> IntoIterator for DelimitedVec<T, D> {
    type Item = Delimited<T, D>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[mutants::skip]
impl<T: std::fmt::Debug, D: std::fmt::Debug> std::fmt::Debug for DelimitedVec<T, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!(
            "DelimitedVec<{}, {}>",
            std::any::type_name::<T>(),
            std::any::type_name::<D>()
        ))
        .field(&self.0)
        .finish()
    }
}

/// Vector of `T` delimited by `,`
pub type CommaDelimitedVec<T> = DelimitedVec<T, Comma>;
/// Vector of `T` delimited by `;`
pub type SemicolonDelimitedVec<T> = DelimitedVec<T, Semicolon>;
/// Vector of `T` delimited by `::`
pub type PathSepDelimitedVec<T> = DelimitedVec<T, PathSep>;
/// Vector of `T` delimited by `.`
pub type DotDelimitedVec<T> = DelimitedVec<T, Dot>;
/// Vector of `T` delimited by `:`
pub type ColonDelimitedVec<T> = DelimitedVec<T, Colon>;

/// Like `DelimitedVec<T,D>` but with a minimum and maximum (inclusive) number of elements.
/// Parsing will succeed when at least the minimum number of elements is reached and stop at
/// the maximum number.  The delimiter `D` defaults to [`Nothing`] to parse sequences which
/// don't have delimiters.
#[derive(Clone)]
pub struct Repeats<const MIN: usize, const MAX: usize, T, D = Nothing>(pub Vec<Delimited<T, D>>);

impl<const MIN: usize, const MAX: usize, T: Parse, D: Parse> Parser for Repeats<MIN, MAX, T, D> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        let mut output = Vec::new();
        while let Ok(delimited) = Delimited::<T, D>::parse(tokens) {
            let done = delimited.delimiter.is_none();
            output.push(delimited);
            #[allow(unused_comparisons)]
            if done || output.len() >= MAX {
                break;
            }
        }

        #[allow(unused_comparisons)]
        if output.len() >= MIN {
            Ok(Self(output))
        } else {
            Error::other(
                tokens,
                format!(
                    "less than MIN Repeats<MIN={MIN}, MAX={MAX}, {}, {}>, got {} repeats",
                    std::any::type_name::<T>(),
                    std::any::type_name::<D>(),
                    output.len()
                ),
            )
        }
    }
}

impl<const MIN: usize, const MAX: usize, T: ToTokens, D: ToTokens> ToTokens
    for Repeats<MIN, MAX, T, D>
{
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.iter().for_each(|value| value.to_tokens(tokens));
    }
}

/// Converts a `[Repeats<MIN, MAX, T, D>`] into a [`Vec<T>`].
/// As with [`DelimitedVec`] this loses the potentially stateful delimiters.
impl<const MIN: usize, const MAX: usize, T, D> From<Repeats<MIN, MAX, T, D>> for Vec<T> {
    fn from(repeats: Repeats<MIN, MAX, T, D>) -> Self {
        repeats
            .0
            .into_iter()
            .map(|delimited| delimited.value)
            .collect()
    }
}

impl<const MIN: usize, const MAX: usize, T, D> IntoIterator for Repeats<MIN, MAX, T, D> {
    type Item = Delimited<T, D>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[mutants::skip]
impl<const MIN: usize, const MAX: usize, T: std::fmt::Debug, D: std::fmt::Debug> std::fmt::Debug
    for Repeats<MIN, MAX, T, D>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!(
            "Repeats<{MIN}, {MAX}, {}, {}>",
            std::any::type_name::<T>(),
            std::any::type_name::<D>()
        ))
        .field(&self.0)
        .finish()
    }
}

/// Any number of T delimited by D or [`Nothing`]
pub type Any<T, D = Nothing> = Repeats<0, { usize::MAX }, T, D>;
/// One or more of T delimited by D or [`Nothing`]
pub type Many<T, D = Nothing> = Repeats<1, { usize::MAX }, T, D>;
/// Zero or one of T delimited by D or [`Nothing`]
pub type Optional<T, D = Nothing> = Repeats<0, 1, T, D>;
/// Exactly N of T delimited by D or [`Nothing`]
pub type Exactly<const N: usize, T, D = Nothing> = Repeats<N, N, T, D>;
/// At most N of T delimited by D or [`Nothing`]
pub type AtMost<const N: usize, T, D = Nothing> = Repeats<0, N, T, D>;
/// At least N of T delimited by D or [`Nothing`]
pub type AtLeast<const N: usize, T, D = Nothing> = Repeats<N, { usize::MAX }, T, D>;

// PLANNED: needs https://github.com/rust-lang/rust/issues/96097 impl<const N: usize, T: Parse> Parser for [T;N] {
