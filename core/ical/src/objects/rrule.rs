use super::*;

/// Recurrence rule.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.5.3>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RRule {
    pub value: Recur,
}

impl From<Recur> for RRule {
    fn from(value: Recur) -> Self {
        Self { value }
    }
}

impl Read<Property> for RRule {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        Some(Self { value: r.value()? })
    }
}

impl Write<Property> for RRule {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.value(&self.value);
    }
}
