use super::*;

/// Priority.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.1.9>
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority {
    value: u8,
}

impl Priority {
    /// Creates a new priority.
    ///
    /// 0 represents an undefined priority, 1 represents the highest priority,
    /// and the lowest priority is 9 (i.e. value passed here must be <= 9).
    #[must_use]
    pub fn new(value: u8) -> Option<Self> {
        if value <= 9 {
            Some(Self { value })
        } else {
            None
        }
    }

    #[must_use]
    pub fn new_unchecked(value: u8) -> Self {
        Self { value }
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.value
    }
}

impl From<u32> for Priority {
    fn from(value: u32) -> Self {
        Priority::new_unchecked(value.min(9) as u8)
    }
}

impl Read<Property> for Priority {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        let value = r.spanned(Reader::value::<u32>)?;
        let (span, value) = (value.span, value.value);

        if value <= 9 {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "we've just checked it's in range"
            )]
            Some(Self { value: value as u8 })
        } else {
            r.error(span, format!("unknown priority `{value}`"));
            None
        }
    }
}

impl Write<Property> for Priority {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.value(self.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(Priority::new(0).unwrap(), ":0")]
    #[test_case(Priority::new(1).unwrap(), ":1")]
    #[test_case(Priority::new(2).unwrap(), ":2")]
    #[test_case(Priority::new(3).unwrap(), ":3")]
    #[test_case(Priority::new(4).unwrap(), ":4")]
    #[test_case(Priority::new(5).unwrap(), ":5")]
    #[test_case(Priority::new(6).unwrap(), ":6")]
    #[test_case(Priority::new(7).unwrap(), ":7")]
    #[test_case(Priority::new(8).unwrap(), ":8")]
    #[test_case(Priority::new(9).unwrap(), ":9")]
    fn smoke(obj: Priority, str: &str) {
        assert_eq!(str, obj.to_string(Property));
        assert_trip!(str, Priority as Property);
    }

    #[test]
    fn constructors() {
        for value in 0..=9 {
            assert_eq!(value, Priority::new(value).unwrap().value);
        }

        for value in 0..=9 {
            assert_eq!(value, Priority::from(u32::from(value)).value);
        }

        assert_eq!(None, Priority::new(10));
        assert_eq!(9, Priority::from(10).value);
    }

    #[test]
    fn unknown() {
        let expected = vec![ReadMsg {
            at: Some(Span::new(1, 3)),
            msg: "unknown priority `10`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        let actual = Priority::from_str(":10", Property).unwrap_err();

        assert_eq!(expected, actual);
    }
}
