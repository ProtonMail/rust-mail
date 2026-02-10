//! A unique feature of unsynn is that one can define a parser as a composition of other
//! parsers on the fly without the need to define custom structures. This is done by using the
//! [`Cons`] and [`Either`] types. The [`Cons`] type is used to define a parser that is a
//! conjunction of two to four other parsers, while the [`Either`] type is used to define a
//! parser that is a disjunction of two to four other parsers.

use crate::{Error, Invalid, Nothing, Parse, Parser, Result, ToTokens, TokenIter, TokenStream};

/// Conjunctive `A` followed by `B` and optional `C` and `D`
/// When `C` and `D` are not used, they are set to [`Nothing`].
#[derive(Clone)]
pub struct Cons<A, B, C = Nothing, D = Nothing> {
    /// The first value
    pub first: A,
    /// The second value
    pub second: B,
    /// The third value
    pub third: C,
    /// The fourth value
    pub fourth: D,
}

impl<A, B, C: 'static, D: 'static> Cons<A, B, C, D> {
    /// Return the number of consecutive used items (not `Nothing`) in a `Cons`
    ///
    /// # Panics
    ///
    /// Asserts that the `Cons` is not sparse, if `C` is not `Nothing` then `D` must not be
    /// `Nothing` either.
    fn used_conjunctions() -> usize {
        let mut len = 2;
        // PLANNED: static NOTHING: TypeId once stable
        let nothing = std::any::TypeId::of::<Nothing>();
        if std::any::TypeId::of::<C>() != nothing {
            len += 1;
        }
        if std::any::TypeId::of::<D>() != nothing {
            assert_ne!(
                std::any::TypeId::of::<C>(),
                nothing,
                "If C is not Nothing then D must be Nothing"
            );
            len += 1;
        }
        len
    }
}

impl<A: Parse, B: Parse, C: Parse, D: Parse> Parser for Cons<A, B, C, D> {
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        Ok(Self {
            first: A::parser(tokens)?,
            second: B::parser(tokens)?,
            third: C::parser(tokens)?,
            fourth: D::parser(tokens)?,
        })
    }
}

impl<A: ToTokens, B: ToTokens, C: ToTokens, D: ToTokens> ToTokens for Cons<A, B, C, D> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.first.to_tokens(tokens);
        self.second.to_tokens(tokens);
        self.third.to_tokens(tokens);
        self.fourth.to_tokens(tokens);
    }
}

impl<A, B> From<Cons<A, B>> for (A, B) {
    fn from(cons: Cons<A, B>) -> Self {
        (cons.first, cons.second)
    }
}

impl<A, B, C> From<Cons<A, B, C>> for (A, B, C) {
    fn from(cons: Cons<A, B, C>) -> Self {
        (cons.first, cons.second, cons.third)
    }
}

impl<A, B, C, D> From<Cons<A, B, C, D>> for (A, B, C, D) {
    fn from(cons: Cons<A, B, C, D>) -> Self {
        (cons.first, cons.second, cons.third, cons.fourth)
    }
}

#[mutants::skip]
impl<A, B, C, D> std::fmt::Debug for Cons<A, B, C, D>
where
    A: std::fmt::Debug,
    B: std::fmt::Debug,
    C: std::fmt::Debug + 'static,
    D: std::fmt::Debug + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match Self::used_conjunctions() {
            1 | 2 => f
                .debug_struct(&format!(
                    "Cons<{}, {}>",
                    std::any::type_name::<A>(),
                    std::any::type_name::<B>(),
                ))
                .field("first", &self.first)
                .field("second", &self.second)
                .finish(),
            3 => f
                .debug_struct(&format!(
                    "Cons<{}, {}, {}>",
                    std::any::type_name::<A>(),
                    std::any::type_name::<B>(),
                    std::any::type_name::<C>(),
                ))
                .field("first", &self.first)
                .field("second", &self.second)
                .field("third", &self.third)
                .finish(),
            _ => f
                .debug_struct(&format!(
                    "Cons<{}, {}, {}, {}>",
                    std::any::type_name::<A>(),
                    std::any::type_name::<B>(),
                    std::any::type_name::<C>(),
                    std::any::type_name::<D>(),
                ))
                .field("first", &self.first)
                .field("second", &self.second)
                .field("third", &self.third)
                .field("fourth", &self.fourth)
                .finish(),
        }
    }
}

/// Disjunctive `A` or `B` or optional `C` or `D`  tried in that order.
/// When `C` and `D` are not used, they are set to [`Invalid`].
#[derive(Clone)]
pub enum Either<A, B, C = Invalid, D = Invalid> {
    /// The first alternative
    First(A),
    /// The second alternative
    Second(B),
    /// The third alternative
    Third(C),
    /// The fourth alternative
    Fourth(D),
}

impl<A, B, C: 'static, D: 'static> Either<A, B, C, D> {
    /// Return the number of consecutive used items (not `Invalid`) in a `Either`
    ///
    /// # Panics
    ///
    /// Asserts that the `Either` is not sparse, if `C` is not `Invalid` then `D` must not be
    /// `Invalid` either.
    fn used_disjunctions() -> usize {
        let mut len = 2;
        // PLANNED: static NOTHING: TypeId once stable
        let invalid = std::any::TypeId::of::<Invalid>();
        if std::any::TypeId::of::<C>() != invalid {
            len += 1;
        }
        if std::any::TypeId::of::<D>() != invalid {
            assert_ne!(
                std::any::TypeId::of::<C>(),
                invalid,
                "If C is not Invalid then D must be Invalid"
            );
            len += 1;
        }
        len
    }

    /// Deconstructs an `Either` with 2 alternatives and produces a common result type, by
    /// applying one of the two functions depending on the alternative.
    ///
    /// # Panics
    ///
    /// When the variant is `Either::Third` or `Either::Fourth`
    ///
    /// # Example
    ///
    /// ```
    /// # use unsynn::*;
    /// let either = Either::<LiteralInteger, Ident>::First(LiteralInteger::new(42));
    /// let result: String = either.fold2(
    ///     |a| a.tokens_to_string(),
    ///     |b| b.tokens_to_string(),
    /// );
    /// assert_eq!(result, "42");
    /// ```
    pub fn fold2<R, FA, FB>(self, first_fn: FA, second_fn: FB) -> R
    where
        FA: FnOnce(A) -> R,
        FB: FnOnce(B) -> R,
    {
        debug_assert_eq!(Self::used_disjunctions(), 2);
        match self {
            Either::First(a) => first_fn(a),
            Either::Second(b) => second_fn(b),
            _ => unimplemented!(),
        }
    }

    /// Deconstructs an `Either` with 3 alternatives and produces a common result type, by
    /// applying one of the three functions depending on the alternative.
    ///
    /// # Panics
    ///
    /// When the variant is `Either::Fourth`
    pub fn fold3<R, FA, FB, FC>(self, first_fn: FA, second_fn: FB, third_fn: FC) -> R
    where
        FA: FnOnce(A) -> R,
        FB: FnOnce(B) -> R,
        FC: FnOnce(C) -> R,
    {
        debug_assert_eq!(Self::used_disjunctions(), 3);
        match self {
            Either::First(a) => first_fn(a),
            Either::Second(b) => second_fn(b),
            Either::Third(c) => third_fn(c),
            Either::Fourth(_) => unimplemented!(),
        }
    }

    /// Deconstructs an `Either` with 4 alternatives and produces a common result type, by
    /// applying one of the provided functions.
    pub fn fold4<R, FA, FB, FC, FD>(
        self,
        first_fn: FA,
        second_fn: FB,
        third_fn: FC,
        fourth_fn: FD,
    ) -> R
    where
        FA: FnOnce(A) -> R,
        FB: FnOnce(B) -> R,
        FC: FnOnce(C) -> R,
        FD: FnOnce(D) -> R,
    {
        debug_assert_eq!(Self::used_disjunctions(), 4);
        match self {
            Either::First(a) => first_fn(a),
            Either::Second(b) => second_fn(b),
            Either::Third(c) => third_fn(c),
            Either::Fourth(d) => fourth_fn(d),
        }
    }

    /// Deconstructs an `Either` with 2 alternatives and produces a common result type for
    /// types that implement `Into<T>`.
    ///
    /// # Panics
    ///
    /// When more then two alternatives are used.
    ///
    /// # Example
    ///
    /// ```
    /// # use unsynn::*;
    /// let either = Either::<LiteralInteger, Ident>::First(LiteralInteger::new(42));
    /// let tt: TokenTree = either.into2();
    /// assert_eq!(tt.tokens_to_string(), "42");
    /// ```
    pub fn into2<T>(self) -> T
    where
        A: Into<T>,
        B: Into<T>,
    {
        debug_assert_eq!(Self::used_disjunctions(), 2);
        match self {
            Either::First(a) => a.into(),
            Either::Second(b) => b.into(),
            _ => unimplemented!(),
        }
    }

    /// Deconstructs an `Either` with 3 alternatives and produces a common result type for types
    /// that implement `Into<T>`.
    ///
    /// # Panics
    ///
    /// When more then three alternatives are used.
    pub fn into3<T>(self) -> T
    where
        A: Into<T>,
        B: Into<T>,
        C: Into<T>,
    {
        debug_assert_eq!(Self::used_disjunctions(), 3);
        match self {
            Either::First(a) => a.into(),
            Either::Second(b) => b.into(),
            Either::Third(c) => c.into(),
            Either::Fourth(_) => unimplemented!(),
        }
    }

    /// Deconstructs an `Either` with 4 alternatives and produces a common result type for types
    /// that implement `Into<T>`.
    pub fn into4<T>(self) -> T
    where
        A: Into<T>,
        B: Into<T>,
        C: Into<T>,
        D: Into<T>,
    {
        debug_assert_eq!(Self::used_disjunctions(), 4);
        match self {
            Either::First(a) => a.into(),
            Either::Second(b) => b.into(),
            Either::Third(c) => c.into(),
            Either::Fourth(d) => d.into(),
        }
    }
}

impl<A, B, C, D> Parser for Either<A, B, C, D>
where
    A: Parse,
    B: Parse,
    C: Parse,
    D: Parse,
{
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        let mut err = Error::no_error();

        if let Ok(first) = err.upgrade(A::parse(tokens)) {
            Ok(Either::First(first))
        } else if let Ok(second) = err.upgrade(B::parse(tokens)) {
            Ok(Either::Second(second))
        } else if let Ok(third) = err.upgrade(C::parse(tokens)) {
            Ok(Either::Third(third))
        } else if let Ok(fourth) = err.upgrade(D::parse(tokens)) {
            Ok(Either::Fourth(fourth))
        } else {
            Err(err)
        }
    }
}

impl<A, B, C, D> ToTokens for Either<A, B, C, D>
where
    A: ToTokens,
    B: ToTokens,
    C: ToTokens,
    D: ToTokens,
{
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Either::First(a) => a.to_tokens(tokens),
            Either::Second(b) => b.to_tokens(tokens),
            Either::Third(c) => c.to_tokens(tokens),
            Either::Fourth(d) => d.to_tokens(tokens),
        }
    }
}

#[mutants::skip]
impl<A, B, C, D> std::fmt::Debug for Either<A, B, C, D>
where
    A: std::fmt::Debug,
    B: std::fmt::Debug,
    C: std::fmt::Debug + 'static,
    D: std::fmt::Debug + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let typename = match Self::used_disjunctions() {
            1 | 2 => format!(
                "Either<{}, {}>",
                std::any::type_name::<A>(),
                std::any::type_name::<B>()
            ),
            3 => format!(
                "Either<{}, {}, {}>",
                std::any::type_name::<A>(),
                std::any::type_name::<B>(),
                std::any::type_name::<C>()
            ),
            _ => format!(
                "Either<{}, {}, {}, {}>",
                std::any::type_name::<A>(),
                std::any::type_name::<B>(),
                std::any::type_name::<C>(),
                std::any::type_name::<D>()
            ),
        };

        match self {
            Either::First(a) => f.debug_tuple(&typename).field(a).finish(),
            Either::Second(b) => f.debug_tuple(&typename).field(b).finish(),
            Either::Third(c) => f.debug_tuple(&typename).field(c).finish(),
            Either::Fourth(d) => f.debug_tuple(&typename).field(d).finish(),
        }
    }
}

#[test]
fn test_either_into_tt() {
    use crate::{LiteralInteger, TokenTree};
    let either = Either::<LiteralInteger, TokenTree>::First(LiteralInteger::new(42));
    let _tt: TokenTree = either.into2();

    let either = Either::<TokenTree, LiteralInteger>::Second(LiteralInteger::new(43));
    let _tt: TokenTree = either.into2();
}

#[cfg(test)]
mod tests {
    use crate::*;
    #[test]
    fn test_cons_used_conjunctions() {
        assert_eq!(Cons::<Punct, Ident>::used_conjunctions(), 2);
        assert_eq!(Cons::<Punct, Ident, LiteralInteger>::used_conjunctions(), 3);
        assert_eq!(
            Cons::<Punct, Ident, LiteralInteger, LiteralCharacter>::used_conjunctions(),
            4
        );
    }

    #[test]
    #[should_panic(expected = "If C is not Nothing then D must be Nothing")]
    fn test_cons_invalid_nothing() {
        Cons::<Punct, Ident, Nothing, LiteralInteger>::used_conjunctions();
    }

    #[test]
    fn test_either_used_disjunctions() {
        assert_eq!(Either::<Punct, Ident>::used_disjunctions(), 2);
        assert_eq!(
            Either::<Punct, Ident, LiteralInteger>::used_disjunctions(),
            3
        );
        assert_eq!(
            Either::<Punct, Ident, LiteralInteger, LiteralCharacter>::used_disjunctions(),
            4
        );
    }

    #[test]
    #[should_panic(expected = "If C is not Invalid then D must be Invalid")]
    fn test_either_invalid_invalid() {
        Either::<Punct, Ident, Invalid, LiteralInteger>::used_disjunctions();
    }
}
