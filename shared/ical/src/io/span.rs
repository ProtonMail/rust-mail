use super::*;
use std::fmt;
use std::ops::Deref;

// 1-indexed, with character offset in *bytes* (not graphemes, not unicode-char-widths)
pub type LineAndChar = (u32, u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: LineAndChar,
    pub end: LineAndChar,
}

impl Span {
    #[must_use]
    pub fn new(start: impl Into<LineAndChar>, end: impl Into<LineAndChar>) -> Self {
        Self {
            start: start.into(),
            end: end.into(),
        }
    }

    #[must_use]
    pub fn one(char: impl Into<LineAndChar>) -> Self {
        let char = char.into();

        Self::new(char, char)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (start_line, start_char) = self.start;
        let (end_line, end_char) = self.end;

        if start_line == end_line {
            if start_char == end_char {
                write!(f, "{start_line}:{start_char}")
            } else {
                write!(f, "{start_line}:{start_char}..{end_char}")
            }
        } else {
            write!(f, "{start_line}:{start_char}..{end_line}:{end_char}")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub span: Span,
    pub value: T,
}

impl<T> Spanned<T> {
    #[must_use]
    pub fn new(span: Span, value: T) -> Self {
        Self { span, value }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            span: self.span,
            value: f(self.value),
        }
    }
}

impl<T, E> Spanned<Result<T, E>>
where
    E: fmt::Display,
{
    pub fn unwrap(self, r: &mut IcsReader) -> Option<T> {
        match self.value {
            Ok(value) => Some(value),

            Err(err) => {
                r.error(self.span, err);
                None
            }
        }
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T, M> IcsRead<M> for Spanned<T>
where
    T: IcsRead<M>,
{
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.spanned(T::read)
    }

    fn name() -> String {
        T::name()
    }
}
