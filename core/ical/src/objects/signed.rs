/// Positive or negative value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signed<T> {
    pub sign: Sign,
    pub value: T,
}

impl<T> Signed<T> {
    #[must_use]
    pub fn new(sign: Sign, value: T) -> Self {
        Self { sign, value }
    }

    #[must_use]
    pub fn neg(value: T) -> Self {
        Self::new(Sign::Neg, value)
    }

    #[must_use]
    pub fn pos(value: T) -> Self {
        Self::new(Sign::Pos, value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    Neg,
    Pos,
}

impl Sign {
    #[must_use]
    pub fn is_neg(&self) -> bool {
        matches!(self, Sign::Neg)
    }

    #[must_use]
    pub fn is_pos(&self) -> bool {
        matches!(self, Sign::Pos)
    }
}
