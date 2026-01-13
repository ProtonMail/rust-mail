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
    /// See: [`Self::undefined()`], [`Self::highest()`], [`Self::lowest()`],
    #[must_use]
    pub fn new(value: u8) -> Self {
        Self {
            value: value.min(9),
        }
    }

    /// Creates the undefined priority (0).
    #[must_use]
    pub fn undefined() -> Self {
        Self { value: 0 }
    }

    /// Creates the highest priority (1).
    #[must_use]
    pub fn highest() -> Self {
        Self { value: 1 }
    }

    /// Creates the lowest priority (9).
    #[must_use]
    pub fn lowest() -> Self {
        Self { value: 9 }
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
        Priority::new(value.min(9) as u8)
    }
}

impl IcsRead<Property> for Priority {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(IcsReader::value::<u32>)?;
        let (span, value) = (value.span, value.value);

        if value <= u32::from(Self::lowest().value) {
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

impl IcsWrite<Property> for Priority {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(self.value);
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Priority {
        const TYPE: PhpDataType = PhpDataType::Long;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            let zval = zval.long()?;
            let zval = u8::try_from(zval).ok()?;

            Some(Self::new(zval))
        }
    }

    impl IntoPhpZval for Priority {
        const TYPE: PhpDataType = PhpDataType::Long;
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, _: bool) -> PhpResult<()> {
            zval.set_long(self.value);

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(Priority::new(0), ":0")]
    #[test_case(Priority::new(1), ":1")]
    #[test_case(Priority::new(2), ":2")]
    #[test_case(Priority::new(3), ":3")]
    #[test_case(Priority::new(4), ":4")]
    #[test_case(Priority::new(5), ":5")]
    #[test_case(Priority::new(6), ":6")]
    #[test_case(Priority::new(7), ":7")]
    #[test_case(Priority::new(8), ":8")]
    #[test_case(Priority::new(9), ":9")]
    fn smoke(obj: Priority, str: &str) {
        assert_eq!(str, obj.to_string(Property));
        assert_trip!(str, Priority as Property);
    }

    #[test]
    fn constructors() {
        for value in 0..=9 {
            assert_eq!(value, Priority::new(value).value);
        }
        for value in 0..=9 {
            assert_eq!(value, Priority::from(u32::from(value)).value);
        }

        assert_eq!(9, Priority::new(10).value);
        assert_eq!(9, Priority::from(10).value);
    }

    #[test]
    fn unknown() {
        let expected = vec![ReadMsg {
            at: Some(Span::new((1, 2), (1, 3))),
            body: "unknown priority `10`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        let actual = Priority::from_str(":10", Property).unwrap_err();

        assert_eq!(expected, actual);
    }
}
