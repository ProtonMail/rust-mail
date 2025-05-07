use super::*;

/// Recurrence rule.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.5.3>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct RRule {
    pub value: Recur,
}

impl RRule {
    pub(crate) fn validate(&self) -> Vec<RRuleViolation> {
        self.value.validate().into_iter().map_into().collect()
    }
}

impl From<Recur> for RRule {
    fn from(value: Recur) -> Self {
        Self { value }
    }
}

impl IcsRead<Property> for RRule {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Property> for RRule {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(&self.value);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RRuleViolation {
    #[error("{0}")]
    InvalidValue(#[from] RecurViolation),
}
