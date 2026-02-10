//! Combined punctuation tokens are represented by [`Operator`]. The [`crate::operator!`]
//! macro can be used to define custom operators.

use crate::{
    Parser, Punct, PunctAny, PunctJoint, Result, Spacing, ToTokens, TokenIter, TokenStream,
};

/// Operators made from up to four ASCII punctuation characters. Unused characters default to `\0`.
/// Custom operators can be defined with the [`crate::operator!`] macro. All but the last character are
/// [`Spacing::Joint`]. Attention must be payed when operators have the same prefix, the shorter
/// ones need to be tried first.
#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub struct Operator<
    const C1: char,
    const C2: char = '\0',
    const C3: char = '\0',
    const C4: char = '\0',
>;

impl<const C1: char, const C2: char, const C3: char, const C4: char> Operator<C1, C2, C3, C4> {
    /// Create a new `Operator` object.
    #[must_use]
    pub const fn new() -> Self {
        const {
            assert!(C1.is_ascii_punctuation());
            assert!(C2 == '\0' || C2.is_ascii_punctuation());
            assert!(C3 == '\0' || C3.is_ascii_punctuation());
            assert!(C4 == '\0' || C4.is_ascii_punctuation());
        }
        Self
    }
}

impl<const C1: char, const C2: char, const C3: char, const C4: char> Parser
    for Operator<C1, C2, C3, C4>
{
    fn parser(tokens: &mut TokenIter) -> Result<Self> {
        if C2 == '\0' {
            PunctAny::<C1>::parser(tokens)?;
            Ok(Self)
        } else {
            PunctJoint::<C1>::parser(tokens)?;
            if C3 == '\0' {
                PunctAny::<C2>::parser(tokens)?;
                Ok(Self)
            } else {
                PunctJoint::<C2>::parser(tokens)?;
                if C4 == '\0' {
                    PunctAny::<C3>::parser(tokens)?;
                    Ok(Self)
                } else {
                    PunctJoint::<C3>::parser(tokens)?;
                    PunctAny::<C4>::parser(tokens)?;
                    Ok(Self)
                }
            }
        }
    }
}

impl<const C1: char, const C2: char, const C3: char, const C4: char> ToTokens
    for Operator<C1, C2, C3, C4>
{
    fn to_tokens(&self, tokens: &mut TokenStream) {
        // Make the spacing `Joint` when the next character is not a space.
        const fn spacing(c: char) -> Spacing {
            if c == '\0' {
                Spacing::Alone
            } else {
                Spacing::Joint
            }
        }

        Punct::new(C1, spacing(C2)).to_tokens(tokens);
        if C2 != '\0' {
            Punct::new(C2, spacing(C3)).to_tokens(tokens);
            if C3 != '\0' {
                Punct::new(C3, spacing(C4)).to_tokens(tokens);
                if C4 != '\0' {
                    Punct::new(C4, Spacing::Alone).to_tokens(tokens);
                }
            }
        }
    }
}

#[mutants::skip]
impl<const C1: char, const C2: char, const C3: char, const C4: char> std::fmt::Debug
    for Operator<C1, C2, C3, C4>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if C4 != '\0' {
            write!(f, "Operator<'{C1}{C2}{C3}{C4}'>")
        } else if C3 != '\0' {
            write!(f, "Operator<'{C1}{C2}{C3}'>")
        } else if C2 != '\0' {
            write!(f, "Operator<'{C1}{C2}'>")
        } else {
            write!(f, "Operator<'{C1}'>")
        }
    }
}

/// Unsynn does not implement rust grammar, for common Operators we make an exception because
/// they are mostly universal and already partial lexed (`Spacing::Alone/Joint`) it would add a
/// lot confusion when every user has to redefine common operator types.  These operator names
/// have their own module and are reexported at the crate root.  This allows one to import
/// only the named operators.
///
/// ```rust
/// # use unsynn::ToTokens;
/// use unsynn::operator::names::*;
/// assert_eq!(Plus::new().tokens_to_string(), "+");
/// ```
pub mod names {
    use crate::{operator, PunctJoint};

    /// `'` With `Spacing::Joint`
    pub type LifetimeTick = PunctJoint<'\''>;

    operator! {
        pub Plus = "+";
        pub Minus = "-";
        pub Star = "*";
        pub Slash = "/";
        pub Percent = "%";
        pub Caret = "^";
        pub Bang = "!";
        pub And = "&";
        pub Or = "|";
        pub AndAnd = "&&";
        pub OrOr = "||";
        pub Shl = "<<";
        pub Shr = ">>";
        pub PlusEq = "+=";
        pub MinusEq = "-=";
        pub StarEq = "*=";
        pub SlashEq = "/=";
        pub PercentEq = "%=";
        pub CaretEq = "^=";
        pub AndEq = "&=";
        pub OrEq = "|=";
        pub ShlEq = "<<=";
        pub ShrEq = ">>=";
        pub Assign = "=";
        pub Equal = "==";
        pub NotEqual = "!=";
        pub Gt = ">";
        pub Lt = "<";
        pub Ge = ">=";
        pub Le = "<=";
        pub At = "@";
        pub Underscore = "_";
        pub Dot = ".";
        pub DotDot = "..";
        pub Ellipsis = "...";
        pub DotDotEq = "..=";
        pub Comma = ",";
        pub Semicolon = ";";
        pub Colon = ":";
        pub PathSep = "::";
        pub RArrow = "->";
        pub FatArrow = "=>";
        pub LArrow = "<-";
        pub Pound = "#";
        pub Dollar = "$";
        pub Question = "?";
        pub Tilde = "~";
        pub Backslash = "\\";
    }
}
