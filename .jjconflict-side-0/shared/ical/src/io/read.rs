use super::*;
use std::str::FromStr;

/// Object that can be deserialized from an *.ics string; see [`IcsReader`].
pub trait IcsRead<M>
where
    Self: Sized,
{
    /// Reads this object from stream, returning `None` if the parsing failed.
    ///
    /// If this method returns `None`, it's expected that it reports some errors
    /// through [`IcsReader::error()`] so that user knows what has failed.
    #[must_use]
    fn read(r: &mut IcsReader) -> Option<Self>;

    /// Returns name of this type, used for diagnostic purposes.
    #[must_use]
    fn name() -> String {
        tynm::type_name::<Self>()
    }

    /// Reasonable default value, if this object couldn't be parsed.
    ///
    /// This is used as an error recovery mechanism for values that are required
    /// by the format, but that can be reasonably assumed to contain an up-front
    /// known value such as `VERSION`.
    #[must_use]
    fn reasonable_default() -> Option<Self> {
        None
    }

    /// Converts string into this object.
    #[track_caller]
    fn from_str(s: &str, marker: M) -> Result<Self, Vec<ReadMsg>> {
        let (this, msgs) = Self::from_str_ex(s, marker);

        if msgs.is_empty() {
            Ok(this.unwrap())
        } else {
            Err(msgs)
        }
    }

    /// Converts string into this object; see [`Self::from_str()`].
    #[track_caller]
    fn from_str_ex(s: &str, _marker: M) -> (Option<Self>, Vec<ReadMsg>) {
        let mut r = IcsReader::new(s.as_bytes());
        let this = Self::read(&mut r);

        assert!(r.is_empty(), "parser left some data unread");

        let msgs = r.finish();

        (this, msgs)
    }
}

impl IcsRead<Value> for char {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.char()
    }
}

impl IcsRead<Value> for u32 {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let number = r.spanned(|r| {
            let mut number = String::new();

            if let Some(s) = r.spanned(|r| r.try_eat('-')) {
                r.error(
                    s.span,
                    "unexpected minus sign, expecting an unsigned integer",
                );
            } else {
                _ = r.try_eat('+');
            }

            while let Some(d) = r.try_digit() {
                number.push(d);
            }

            Some(number)
        })?;

        match <u32 as FromStr>::from_str(&number.value) {
            Ok(val) => Some(val),

            Err(err) => {
                r.error(number.span, err);
                None
            }
        }
    }
}

impl<T> IcsRead<Value> for Vec<T>
where
    T: IcsRead<Value>,
{
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut values = Vec::new();

        r.hint(
            |h| {
                h.inside_array = true;
            },
            |r| {
                loop {
                    if let Some(value) = r.value() {
                        values.push(value);
                    }

                    if r.try_eat(',').is_none() {
                        break;
                    }
                }
            },
        );

        Some(values)
    }
}
