use super::*;

/// Time zone; part of a [`VCalendar`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.6.5>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VTimeZone {
    pub tzid: TzId,
    pub daylights: Vec<TzProps>,
    pub standards: Vec<TzProps>,
}

impl VTimeZone {
    #[must_use]
    pub fn new(tzid: impl Into<TzId>, daylight: TzProps, standard: TzProps) -> Self {
        Self {
            tzid: tzid.into(),
            daylights: vec![daylight],
            standards: vec![standard],
        }
    }

    #[must_use]
    pub fn new_daylight(tzid: impl Into<TzId>, props: TzProps) -> Self {
        Self {
            tzid: tzid.into(),
            daylights: vec![props],
            standards: Vec::new(),
        }
    }

    #[must_use]
    pub fn new_standard(tzid: impl Into<TzId>, props: TzProps) -> Self {
        Self {
            tzid: tzid.into(),
            daylights: Vec::new(),
            standards: vec![props],
        }
    }

    #[must_use]
    pub fn with_daylight(mut self, props: TzProps) -> Self {
        self.daylights.push(props);
        self
    }

    #[must_use]
    pub fn with_standard(mut self, props: TzProps) -> Self {
        self.standards.push(props);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VTimeZoneViolation {
    #[error("id `{0}` is already taken")]
    DuplicatedId(String),
}

/// Time zone properties; part of a [`VTimeZone`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TzProps {
    pub dtstart: DtStart,
    pub tz_offset_from: TzOffsetFrom,
    pub tz_offset_to: TzOffsetTo,
    pub rrule: Option<RRule>,
    pub tz_name: Option<TzName>,
}

impl TzProps {
    #[must_use]
    pub fn new(
        dtstart: impl Into<DtStart>,
        tz_offset_from: impl Into<TzOffsetFrom>,
        tz_offset_to: impl Into<TzOffsetTo>,
    ) -> Self {
        Self {
            dtstart: dtstart.into(),
            tz_offset_from: tz_offset_from.into(),
            tz_offset_to: tz_offset_to.into(),
            rrule: None,
            tz_name: None,
        }
    }

    #[must_use]
    pub fn with_rrule(mut self, rrule: impl Into<RRule>) -> Self {
        self.rrule = Some(rrule.into());
        self
    }

    #[must_use]
    pub fn with_tz_name(mut self, tz_name: impl Into<TzName>) -> Self {
        self.tz_name = Some(tz_name.into());
        self
    }
}
