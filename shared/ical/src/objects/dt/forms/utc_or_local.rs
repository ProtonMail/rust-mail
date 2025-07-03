use super::*;

/// Marks a UTC or local [`DateTime`]; subset of [`AnyForm`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtcOrLocalForm {
    Local,
    Utc,
}

impl From<UtcOrLocalForm> for AnyForm {
    fn from(value: UtcOrLocalForm) -> Self {
        match value {
            UtcOrLocalForm::Local => AnyForm::Local,
            UtcOrLocalForm::Utc => AnyForm::Utc,
        }
    }
}

impl TryFrom<AnyForm> for UtcOrLocalForm {
    type Error = DateTimeError;

    fn try_from(value: AnyForm) -> Result<Self, Self::Error> {
        match value {
            AnyForm::Local => Ok(UtcOrLocalForm::Local),
            AnyForm::Utc => Ok(UtcOrLocalForm::Utc),

            AnyForm::Tz(_) => Err(DateTimeError::InvalidConversion(
                value.ty(),
                "local-form or utc-form",
            )),
        }
    }
}

impl From<DateTime<UtcOrLocalForm>> for DateTime {
    fn from(value: DateTime<UtcOrLocalForm>) -> Self {
        Self {
            date: value.date,
            time: value.time,
            form: value.form.into(),
        }
    }
}

impl TryFrom<DateTime<AnyForm>> for DateTime<UtcOrLocalForm> {
    type Error = DateTimeError;

    fn try_from(value: DateTime<AnyForm>) -> Result<Self, Self::Error> {
        Ok(Self {
            date: value.date,
            time: value.time,
            form: value.form.try_into()?,
        })
    }
}

impl TryFrom<JiffZoned> for DateTime<UtcOrLocalForm> {
    type Error = DateTimeError;

    fn try_from(value: JiffZoned) -> Result<Self, Self::Error> {
        DateTime::<AnyForm>::try_from(value)?.try_into()
    }
}

impl TryFrom<DateTime<UtcOrLocalForm>> for JiffZoned {
    type Error = DateTimeError;

    fn try_from(value: DateTime<UtcOrLocalForm>) -> Result<Self, Self::Error> {
        DateTime::<AnyForm>::from(value).try_into()
    }
}

impl IcsRead<Value> for DateTime<UtcOrLocalForm> {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let date = r.value()?;
        r.eat('T')?;
        let time = r.value()?;

        let form = if r.try_eat('Z').is_some() {
            UtcOrLocalForm::Utc
        } else {
            UtcOrLocalForm::Local
        };

        Some(Self { date, time, form })
    }
}

impl IcsWrite<Value> for DateTime<UtcOrLocalForm> {
    fn write(&self, w: &mut IcsWriter) {
        w.value(self.date);
        w.raw("T");
        w.value(self.time);

        if let UtcOrLocalForm::Utc = self.form {
            w.raw("Z");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_trip!("20180101T123456", DateTime<UtcOrLocalForm> as Value);
        assert_trip!("20180101T123456Z", DateTime<UtcOrLocalForm> as Value);
    }
}
