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

impl IcsRead<Value> for DtValueType {
    fn read(r: &mut IcsReader) -> Option<Self> {
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

impl IcsWrite<Value> for DtValueType {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            DtValueType::Date => "DATE",
            DtValueType::DateTime => "DATE-TIME",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;
    use ext_php_rs::{boxed::ZBox, types::ZendObject, zend::ClassEntry};

    fn create_date_time() -> PhpResult<ZBox<ZendObject>> {
        // Unwrap-safety: `DateTimeImmutable` is part of the stdlib
        let dt = ZendObject::new(ClassEntry::try_find("DateTimeImmutable").unwrap());

        dt.try_call_method("__construct", Vec::new())?;

        Ok(dt)
    }

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

    fn create_timestamp(jiff: &JiffZoned) -> PhpZval {
        let mut arg = PhpZval::new();

        arg.set_long(jiff.timestamp().as_second());
        arg
    }

    impl<'a, F> FromPhpZval<'a> for DateTime<F>
    where
        DateTime<F>: TryFrom<JiffZoned, Error = DateTimeError>,
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

            dt.try_into().ok()
        }
    }

    impl<F> IntoPhpZval for DateTime<F>
    where
        DateTime<F>: TryInto<JiffZoned, Error = DateTimeError>,
    {
        const TYPE: PhpDataType = PhpDataType::Object(Some("DateTimeImmutable"));
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            // TODO avoid unwrapping
            let jiff: JiffZoned = self.try_into().unwrap();

            let dt = create_date_time()?;
            let tz = create_time_zone(jiff.time_zone(), persistent)?;
            let ts = create_timestamp(&jiff);

            let dt = dt.try_call_method("setTimezone", vec![&tz])?;
            let dt = dt.try_call_method("setTimestamp", vec![&ts])?;

            *zval = dt;

            Ok(())
        }
    }
}
