use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrDt<F = AnyForm> {
    Date(Date),
    DateTime(DateTime<F>),
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

impl Read<Property> for DateOrDt {
    fn read(r: &mut Reader) -> Option<Self> {
        let mut tzid = None;
        let mut value = None;

        while let Some(e) = r.entry() {
            if e.try_param(r, "TZID", &mut tzid) || e.try_param(r, "VALUE", &mut value) {
                continue;
            }

            e.burn(r);
        }

        r.eat(':')?;

        match value.unwrap_or_default() {
            DtValueType::Date => {
                let date = r.value()?;

                if let Some(s) = r.spanned(|r| r.try_eat('T')) {
                    if let Some(s) = r.spanned(|r| r.try_string("000000")) {
                        r.warn(
                            s.span,
                            "non-conformant: skipping T000000 to coerce this \
                             date-time into date",
                        );
                    } else {
                        r.error(s.span, "unexpected time component");

                        _ = r.silently(Reader::value::<Time>);
                    }
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

impl Write<Property> for DateOrDt {
    fn write(&self, w: &mut Writer) {
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

impl<F> Read<Value> for DateOrDt<F>
where
    DateTime<F>: Read<Value>,
{
    fn read(r: &mut Reader) -> Option<Self> {
        if let Some(value) = r.attempt(Reader::value) {
            Some(DateOrDt::DateTime(value))
        } else {
            Some(DateOrDt::Date(r.value()?))
        }
    }
}

impl<F> Write<Value> for DateOrDt<F>
where
    DateTime<F>: Write<Value>,
{
    fn write(&self, w: &mut Writer) {
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
