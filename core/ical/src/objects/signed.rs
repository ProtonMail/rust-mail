use super::*;

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

impl<T> Read<Value> for Signed<T>
where
    T: Read<Value>,
{
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self {
            sign: r.value()?,
            value: r.value()?,
        })
    }
}

impl<T> Write<Value> for Signed<T>
where
    T: Write<Value>,
{
    fn write(&self, w: &mut Writer) {
        match self.sign {
            Sign::Neg => {
                self.sign.write(w);
            }
            Sign::Pos => {
                // Implementations tend to omit the default `Sign::Pos`
            }
        }

        self.value.write(w);
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

impl Read<Value> for Sign {
    fn read(r: &mut Reader) -> Option<Self> {
        if r.try_eat('-').is_some() {
            Some(Sign::Neg)
        } else {
            _ = r.try_eat('+');
            Some(Sign::Pos)
        }
    }
}

impl Write<Value> for Sign {
    fn write(&self, w: &mut Writer) {
        w.raw(match self {
            Sign::Neg => "-",
            Sign::Pos => "+",
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("-MO" ; "neg")]
    #[test_case("MO" ; "pos")]
    fn smoke(s: &str) {
        assert_trip!(s, Signed::<Weekday> as Value);
    }
}
