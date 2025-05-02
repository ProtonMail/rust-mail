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

impl Read<Property> for DateTime<AnyForm> {
    fn read(r: &mut Reader) -> Option<Self> {
        let mut tzid = None;

        while let Some(e) = r.entry() {
            if e.try_param(r, "TZID", &mut tzid) {
                continue;
            }

            e.burn(r);
        }

        r.eat(':')?;
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

impl Write<Property> for DateTime<AnyForm> {
    fn write(&self, w: &mut Writer) {
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
