use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrDt<F = AnyForm> {
    Date(Date),
    DateTime(DateTime<F>),
}

impl<F> DateOrDt<F> {
    #[must_use]
    pub(crate) fn ty(&self) -> &'static str {
        match self {
            DateOrDt::Date(_) => "date",
            DateOrDt::DateTime(_) => "date-time",
        }
    }
}

impl DateOrDt<AnyForm> {
    #[must_use]
    pub fn tzid(&self) -> Option<&TzId> {
        match self {
            DateOrDt::Date(_) => None,
            DateOrDt::DateTime(dt) => match &dt.form {
                AnyForm::Local | AnyForm::Utc => None,
                AnyForm::Tz(tz) => Some(tz),
            },
        }
    }

    #[must_use]
    pub(crate) fn validate(&self, cal: &VCalendar) -> Vec<DateTimeViolation> {
        match self {
            DateOrDt::Date(_) => Vec::new(),
            DateOrDt::DateTime(this) => this.validate(cal),
        }
    }
}

impl From<Date> for DateOrDt {
    fn from(value: Date) -> Self {
        DateOrDt::Date(value)
    }
}

impl<F> From<DateTime<F>> for DateOrDt<F> {
    fn from(value: DateTime<F>) -> Self {
        DateOrDt::DateTime(value)
    }
}

impl<F> TryFrom<DateOrDt<F>> for JiffZoned
where
    DateTime<F>: TryInto<JiffZoned, Error = DateTimeError>,
{
    type Error = DateTimeError;

    fn try_from(value: DateOrDt<F>) -> Result<Self, Self::Error> {
        match value {
            DateOrDt::Date(value) => value.try_into(),
            DateOrDt::DateTime(value) => value.try_into(),
        }
    }
}

impl IcsRead<Property> for DateOrDt {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut tzid = None;
        let mut value = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "TZID", &mut tzid) || e.try_param(r, "VALUE", &mut value) {
                continue;
            }

            if e.is_value() {
                break;
            }

            e.burn(r, Kind::Property)?;
        }

        match value.unwrap_or_default() {
            DtValueType::Date => {
                let date = r.value()?;

                if let Some(s) = r.spanned(|r| r.try_string("T000000")) {
                    r.warn(s.span, "quirky time component");
                } else if let Some(s) = r.spanned(|r| r.try_eat('T')) {
                    r.error(s.span, "unexpected time component");

                    _ = r.silently(IcsReader::value::<Time>);
                }

                Some(DateOrDt::Date(date))
            }

            DtValueType::DateTime => {
                let dt = if let Some(tzid) = tzid {
                    let date = r.value()?;
                    r.eat('T')?;
                    let time = r.value()?;

                    if let Some(s) = r.spanned(|r| r.try_eat('Z')) {
                        r.error(s.span, "`Z` cannot be specified together with `TZID`");
                    }

                    DateTime {
                        date,
                        time,
                        form: AnyForm::Tz(tzid),
                    }
                } else {
                    DateTime::from(r.value::<DateTime<UtcOrLocalForm>>()?)
                };

                Some(DateOrDt::DateTime(dt))
            }
        }
    }
}

impl IcsWrite<Property> for DateOrDt {
    fn write(&self, w: &mut IcsWriter) {
        match self {
            DateOrDt::Date(this) => {
                w.param("VALUE", DtValueType::Date);
                w.raw(":");
                this.write(w);
            }
            DateOrDt::DateTime(this) => {
                // Implied VALUE=DATE-TIME
                this.write(w);
            }
        }
    }
}

impl<F> IcsRead<Value> for DateOrDt<F>
where
    DateTime<F>: IcsRead<Value>,
{
    fn read(r: &mut IcsReader) -> Option<Self> {
        if let Some(value) = r.attempt(IcsReader::value) {
            Some(DateOrDt::DateTime(value))
        } else {
            Some(DateOrDt::Date(r.value()?))
        }
    }
}

impl<F> IcsWrite<Value> for DateOrDt<F>
where
    DateTime<F>: IcsWrite<Value>,
{
    fn write(&self, w: &mut IcsWriter) {
        match self {
            DateOrDt::Date(this) => {
                this.write(w);
            }
            DateOrDt::DateTime(this) => {
                this.write(w);
            }
        }
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;
    use ext_php_rs::types::ZendObject;

    impl<'a, F> FromPhpZval<'a> for DateOrDt<F>
    where
        DateTime<F>: FromPhpZval<'a>,
    {
        const TYPE: PhpDataType = PhpDataType::Object(None);

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            let zval = zval.object()?;

            match zval.get_property("kind").ok()? {
                "Date" => Some(DateOrDt::Date(zval.get_property("date").ok()?)),
                "DateTime" => Some(DateOrDt::DateTime(zval.get_property("date").ok()?)),
                _ => None,
            }
        }
    }

    impl<F> IntoPhpZval for DateOrDt<F>
    where
        DateTime<F>: IntoPhpZval,
    {
        const TYPE: PhpDataType = PhpDataType::Object(None);
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            let mut obj = ZendObject::new_stdclass();

            match self {
                DateOrDt::Date(this) => {
                    obj.set_property("kind", "Date")?;
                    obj.set_property("date", this)?;
                }

                DateOrDt::DateTime(this) => {
                    obj.set_property("kind", "DateTime")?;
                    obj.set_property("date", this)?;
                }
            }

            obj.set_zval(zval, persistent)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_trip!(";VALUE=DATE:20180101", DateOrDt as Property);
        assert_trip!(":20180101T120000Z", DateOrDt as Property);
        assert_trip!(";TZID=Europe/Warsaw:20180101T120000", DateOrDt as Property);

        assert_trip!(
            ";VALUE=DATE:20180101T000000" => ";VALUE=DATE:20180101", yielding [
                ReadMsg {
                    at: Some(Span::new((1, 21), (1, 27))),
                    body: "quirky time component".into(),
                    kind: ReadMsgKind::Warning,
                    context: Vec::new(),
                },
            ],
            DateOrDt as Property
        );

        assert_trip!(
            ";VALUE=DATE:20180101T123456" => ";VALUE=DATE:20180101", yielding [
                ReadMsg {
                    at: Some(Span::new((1, 21), (1, 21))),
                    body: "unexpected time component".into(),
                    kind: ReadMsgKind::Error,
                    context: Vec::new(),
                },
            ],
            DateOrDt as Property
        );
    }
}
