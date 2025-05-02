use super::*;

/// Interpretation of [`DateTime`]'s time component; subset of [`AnyForm`].
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

impl From<DateTime<UtcOrLocalForm>> for DateTime {
    fn from(value: DateTime<UtcOrLocalForm>) -> Self {
        Self {
            date: value.date,
            time: value.time,
            form: value.form.into(),
        }
    }
}

impl AsJiffZoned for DateTime<UtcOrLocalForm> {
    fn as_jiff(&self) -> Result<JiffZoned, JiffError> {
        DateTime::<AnyForm>::from(*self).as_jiff()
    }
}

impl Read<Value> for DateTime<UtcOrLocalForm> {
    fn read(r: &mut Reader) -> Option<Self> {
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

impl Write<Value> for DateTime<UtcOrLocalForm> {
    fn write(&self, w: &mut Writer) {
        w.value(self.date);
        w.raw("T");
        w.value(self.time);

        if let UtcOrLocalForm::Utc = self.form {
            w.raw("Z");
        }
    }
}
