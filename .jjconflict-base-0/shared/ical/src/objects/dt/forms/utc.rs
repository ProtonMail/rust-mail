use super::*;

/// Marks a UTC-only [`DateTime`]; subset of [`AnyForm`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtcForm;

impl From<UtcForm> for AnyForm {
    fn from(_: UtcForm) -> Self {
        AnyForm::Utc
    }
}

impl TryFrom<AnyForm> for UtcForm {
    type Error = DateTimeError;

    fn try_from(value: AnyForm) -> Result<Self, Self::Error> {
        if value == AnyForm::Utc {
            Ok(Self)
        } else {
            Err(DateTimeError::InvalidConversion(value.ty(), "utc-form"))
        }
    }
}

impl From<DateTime<UtcForm>> for DateTime<AnyForm> {
    fn from(value: DateTime<UtcForm>) -> Self {
        Self {
            date: value.date,
            time: value.time,
            form: value.form.into(),
        }
    }
}

impl TryFrom<DateTime<AnyForm>> for DateTime<UtcForm> {
    type Error = DateTimeError;

    fn try_from(value: DateTime<AnyForm>) -> Result<Self, Self::Error> {
        Ok(Self {
            date: value.date,
            time: value.time,
            form: value.form.try_into()?,
        })
    }
}

impl TryFrom<JiffZoned> for DateTime<UtcForm> {
    type Error = DateTimeError;

    fn try_from(value: JiffZoned) -> Result<Self, Self::Error> {
        DateTime::<AnyForm>::try_from(value)?.try_into()
    }
}

impl TryFrom<DateTime<UtcForm>> for JiffZoned {
    type Error = DateTimeError;

    fn try_from(value: DateTime<UtcForm>) -> Result<Self, Self::Error> {
        DateTime::<AnyForm>::from(value).try_into()
    }
}

impl IcsRead<Value> for DateTime<UtcForm> {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let date = r.value()?;
        r.eat('T')?;
        let time = r.value()?;

        if r.try_eat('Z').is_none() {
            r.warn(
                Span::one(r.pos()),
                "expected utc-date-time (missing `Z` here)",
            );
        }

        Some(Self {
            date,
            time,
            form: UtcForm,
        })
    }
}

impl IcsWrite<Value> for DateTime<UtcForm> {
    fn write(&self, w: &mut IcsWriter) {
        w.value(DateTime {
            date: self.date,
            time: self.time,
            form: UtcOrLocalForm::Utc,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_trip!("20180101T123456Z", DateTime<UtcForm> as Value);

        assert_trip!(
            "20180101T123456" => "20180101T123456Z", yielding [
                ReadMsg {
                    at: Some(Span::new((1, 16), (1, 16))),
                    body: "expected utc-date-time (missing `Z` here)".into(),
                    kind: ReadMsgKind::Warning,
                    context: Vec::new(),
                },
            ],
            DateTime<UtcForm> as Value
        );
    }
}
