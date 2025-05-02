use super::*;

/// Interpretation of [`DateTime`]'s time component; subset of [`AnyForm`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtcForm;

impl From<UtcForm> for AnyForm {
    fn from(_: UtcForm) -> Self {
        AnyForm::Utc
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

impl AsJiffZoned for DateTime<UtcForm> {
    fn as_jiff(&self) -> Result<JiffZoned, JiffError> {
        DateTime::<AnyForm>::from(*self).as_jiff()
    }
}

impl Read<Value> for DateTime<UtcForm> {
    fn read(r: &mut Reader) -> Option<Self> {
        let date = r.value()?;
        r.eat('T')?;
        let time = r.value()?;

        if r.try_eat('Z').is_none() {
            r.warn(
                Span::new(r.pos(), r.pos() + 1),
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

impl Write<Value> for DateTime<UtcForm> {
    fn write(&self, w: &mut Writer) {
        w.value(DateTime {
            date: self.date,
            time: self.time,
            form: UtcOrLocalForm::Utc,
        });
    }
}
