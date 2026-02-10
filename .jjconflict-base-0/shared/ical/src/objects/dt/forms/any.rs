use super::*;

/// Interpretation of [`DateTime`]'s time component.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnyForm {
    Local,
    Utc,
    Tz(TzId),
}

impl AnyForm {
    #[must_use]
    pub(crate) fn ty(&self) -> &'static str {
        match self {
            AnyForm::Local => "local-form",
            AnyForm::Utc => "utc-form",
            AnyForm::Tz(_) => "tz-form",
        }
    }
}

impl DateTime<AnyForm> {
    #[must_use]
    pub(crate) fn validate(&self, cal: &VCalendar) -> Vec<DateTimeViolation> {
        let mut viols = Vec::new();

        if let AnyForm::Tz(tzid) = &self.form {
            if !cal.timezones.iter().any(|tz| tz.tzid.value == tzid.value) {
                viols.push(DateTimeViolation::UnknownTimeZone(tzid.value.clone()));
            }
        }

        viols
    }
}

impl TryFrom<JiffZoned> for DateTime<AnyForm> {
    type Error = DateTimeError;

    fn try_from(value: JiffZoned) -> Result<Self, Self::Error> {
        let tz = value.time_zone();

        let form = if tz.is_unknown() {
            AnyForm::Local
        } else if *tz == JiffTimeZone::UTC {
            AnyForm::Utc
        } else {
            let tz = tz
                .iana_name()
                .ok_or_else(|| DateTimeError::UnknownTimeZone(value.clone()))?
                .to_owned();

            AnyForm::Tz(TzId::from(tz))
        };

        Ok(Self {
            date: value.date().into(),
            time: value.time().into(),
            form,
        })
    }
}

impl TryFrom<DateTime<AnyForm>> for JiffZoned {
    type Error = DateTimeError;

    fn try_from(value: DateTime<AnyForm>) -> Result<Self, Self::Error> {
        let dt = JiffDateTime::from_parts(value.date.into(), value.time.into());

        match value.form {
            AnyForm::Local => Ok(dt.to_zoned(JiffTimeZone::unknown())?),
            AnyForm::Utc => Ok(dt.to_zoned(JiffTimeZone::UTC)?),

            AnyForm::Tz(tz) => {
                let result = dt.in_tz(tz.value.as_str());

                // TODO use let-chains once we bump php-ical to a newer toolchain
                if result.is_err() {
                    if let Some(tz) = proton_ical_tz::windows_to_tzdb(tz.value.as_str()) {
                        if let Ok(this) = dt.in_tz(tz) {
                            return Ok(this);
                        }
                    }
                }

                result.map_err(Into::into)
            }
        }
    }
}

impl IcsRead<Property> for DateTime<AnyForm> {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut tzid = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "TZID", &mut tzid) {
                continue;
            }

            if e.is_value() {
                break;
            }

            e.burn(r, Kind::Property)?;
        }

        let date = r.value()?;
        r.eat('T')?;
        let time = r.value()?;

        let form = if r.try_eat('Z').is_some() {
            AnyForm::Utc
        } else if let Some(tzid) = tzid {
            AnyForm::Tz(tzid)
        } else {
            AnyForm::Local
        };

        Some(Self { date, time, form })
    }
}

impl IcsWrite<Property> for DateTime<AnyForm> {
    fn write(&self, w: &mut IcsWriter) {
        if let AnyForm::Tz(tzid) = &self.form {
            w.param("TZID", tzid);
        }

        w.raw(":");
        w.value(self.date);
        w.raw("T");
        w.value(self.time);

        if let AnyForm::Utc = &self.form {
            w.raw("Z");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;

    #[test]
    fn jiff() {
        let jiff = jz("2018-01-02 12:34:56+00:00[Etc/Unknown]");

        let us = DateTime {
            date: Date::new_unchecked(2018, 1, 2),
            time: Time::new_unchecked(12, 34, 56),
            form: AnyForm::Local,
        };

        assert_eq!(us, DateTime::try_from(jiff.clone()).unwrap());
        assert_eq!(jiff, JiffZoned::try_from(us).unwrap());

        // ---

        let jiff = jz("2018-01-02 12:34:56");

        let us = DateTime {
            date: Date::new_unchecked(2018, 1, 2),
            time: Time::new_unchecked(12, 34, 56),
            form: AnyForm::Utc,
        };

        assert_eq!(us, DateTime::try_from(jiff.clone()).unwrap());
        assert_eq!(jiff, JiffZoned::try_from(us).unwrap());

        // ---

        let jiff = jz("2018-01-02 12:34:56+01:00[Europe/Stockholm]");

        let us = DateTime {
            date: Date::new_unchecked(2018, 1, 2),
            time: Time::new_unchecked(12, 34, 56),
            form: AnyForm::Tz("Europe/Stockholm".into()),
        };

        assert_eq!(us, DateTime::try_from(jiff.clone()).unwrap());
        assert_eq!(jiff, JiffZoned::try_from(us).unwrap());
    }

    #[test]
    fn smoke() {
        assert_trip!(":20180101T123456", DateTime as Property);
        assert_trip!(":20180101T123456Z", DateTime as Property);
        assert_trip!(";TZID=Europe/Warsaw:20180101T123456", DateTime as Property);
    }
}
