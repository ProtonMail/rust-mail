use super::*;

/// Date, time, and the associated form (describing the time zone etc.).
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime<F = AnyForm> {
    pub date: Date,
    pub time: Time,
    pub form: F,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DtValueType {
    Date,
    #[default]
    DateTime,
}

impl Read<Value> for DtValueType {
    fn read(r: &mut Reader) -> Option<Self> {
        let value = r.value::<Spanned<ParamValue>>()?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("DATE") {
            Some(DtValueType::Date)
        } else if value.eq_ignore_ascii_case("DATE-TIME") {
            Some(DtValueType::DateTime)
        } else {
            r.error(span, format!("unknown value `{value}`"));
            None
        }
    }
}

impl Write<Value> for DtValueType {
    fn write(&self, w: &mut Writer) {
        w.raw(match self {
            DtValueType::Date => "DATE",
            DtValueType::DateTime => "DATE-TIME",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;
    use ext_php_rs::{types::ZendObject, zend::ClassEntry};

    fn create_time_zone(tz: &JiffTimeZone, persistent: bool) -> PhpResult<PhpZval> {
        let name = {
            let mut zval = PhpZval::new();

            zval.set_string(tz.iana_name().unwrap_or("UTC"), persistent)?;
            zval
        };

        let obj = {
            // Unwrap-safety: `DateTimeZone` is part of the stdlib
            let tz = ZendObject::new(ClassEntry::try_find("DateTimeZone").unwrap());

            tz.try_call_method("__construct", vec![&name])?;
            tz
        };

        let mut zval = PhpZval::new();

        obj.set_zval(&mut zval, persistent)?;

        Ok(zval)
    }

    impl<'a, F> FromPhpZval<'a> for DateTime<F>
    where
        DateTime<F>: FromJiffZoned,
    {
        const TYPE: PhpDataType = PhpDataType::Object(Some("DateTimeImmutable"));

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            let ts = zval
                .try_call_method("getTimestamp", Vec::new())
                .ok()?
                .long()?;

            let tz = zval
                .try_call_method("getTimezone", Vec::new())
                .ok()?
                .try_call_method("getName", Vec::new())
                .ok()?;

            let dt = jiff::Timestamp::from_second(ts)
                .ok()?
                .in_tz(tz.str()?)
                .ok()?;

            Self::from_jiff(dt)
        }
    }

    impl<F> IntoPhpZval for DateTime<F>
    where
        DateTime<F>: AsJiffZoned,
    {
        const TYPE: PhpDataType = PhpDataType::Object(Some("DateTimeImmutable"));

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            // TODO avoid unwrapping
            let jiff = self.as_jiff().unwrap();

            let dt = {
                // Unwrap-safety: `DateTimeImmutable` is part of the stdlib
                let dt = ZendObject::new(ClassEntry::try_find("DateTimeImmutable").unwrap());

                dt.try_call_method("__construct", Vec::new())?;
                dt
            };

            let tz = create_time_zone(jiff.time_zone(), persistent)?;

            let ts = {
                let mut arg = PhpZval::new();

                arg.set_long(jiff.timestamp().as_second());
                arg
            };

            let dt = dt.try_call_method("setTimezone", vec![&tz])?;
            let dt = dt.try_call_method("setTimestamp", vec![&ts])?;

            *zval = dt;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_trip!(":20180101T123456", DateTime as Property);
        assert_trip!(":20180101T123456Z", DateTime as Property);
        assert_trip!(";TZID=Europe/Warsaw:20180101T123456", DateTime as Property);

        // ---

        assert_trip!("20180101T123456", DateTime<UtcOrLocalForm> as Value);
        assert_trip!("20180101T123456Z", DateTime<UtcOrLocalForm> as Value);

        // ---

        assert_trip!("20180101T123456Z", DateTime<UtcForm> as Value);

        assert_trip!(
            "20180101T123456" => "20180101T123456Z", yielding [
                ReadMsg {
                    at: Some(Span::new(15, 16)),
                    msg: "expected utc-date-time (missing `Z` here)".into(),
                    kind: ReadMsgKind::Warning,
                    context: Vec::new(),
                },
            ],
            DateTime<UtcForm> as Value
        );
    }
}
