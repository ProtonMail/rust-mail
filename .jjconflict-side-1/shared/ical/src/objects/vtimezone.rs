use super::*;

/// Time zone; part of a [`VCalendar`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.6.5>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

    /// Converts given string into a time zone.
    ///
    /// This comes handy for parsing externally-provided time zones[1], but note
    /// that most of the time what you really want is [`VCalendar::from_str()`].
    ///
    /// [1] <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/VTimezone/operation/get_calendar-%7B_version%7D-vtimezones>
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(src: &str) -> Result<ParsedVTimeZone> {
        let mut r = IcsReader::new(src.as_bytes());
        let mut tz: Option<Self> = None;

        while !r.is_empty() {
            let Some(e) = r.entry() else {
                break;
            };

            if !e.try_comp(&mut r, "VTIMEZONE", &mut tz) {
                _ = e.burn(&mut r, Kind::Component);
            }
        }

        let mut msgs = r.finish();

        let Some(tz) = tz else {
            if msgs.iter().any(|msg| msg.kind.is_error()) {
                // If we've already gotten some errors, let's avoid piling up an
                // extra "missing time zone" - that's because most likely the
                // reason why we're missing the time zone *is* one of those
                // other errors (e.g. there's an invalid syntax somewhere)
            } else {
                msgs.push(ReadMsg {
                    at: None,
                    body: "missing time zone".into(),
                    kind: ReadMsgKind::Error,
                    context: Vec::new(),
                });
            }

            return Err(Error::InvalidIcs(msgs));
        };

        Ok(ParsedVTimeZone { tz, msgs })
    }

    #[must_use]
    pub(crate) fn validate(
        &self,
        cal: &VCalendar,
        skipping_idx: Option<usize>,
    ) -> Vec<VTimeZoneViolation> {
        let mut viols = Vec::new();

        if cal
            .timezones
            .iter()
            .enumerate()
            .any(|(idx, tz)| Some(idx) != skipping_idx && tz.tzid == self.tzid)
        {
            viols.push(VTimeZoneViolation::DuplicatedId(
                self.tzid.value.as_str().to_owned(),
            ));
        }

        viols
    }
}

impl IcsRead<Component> for VTimeZone {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut tzid = None;
        let mut daylights = Vec::new();
        let mut standards = Vec::new();

        loop {
            let e = r.entry()?;

            if e.try_prop(r, "TZID", &mut tzid)
                || e.try_comps(r, "DAYLIGHT", &mut daylights)
                || e.try_comps(r, "STANDARD", &mut standards)
            {
                continue;
            }

            if e.try_comp_end("VTIMEZONE") {
                break;
            }

            e.burn(r, Kind::Component)?;
        }

        let tzid = r.unwrap_prop("TZID", tzid);

        Some(Self {
            tzid: tzid?,
            daylights,
            standards,
        })
    }
}

impl IcsWrite<Component> for VTimeZone {
    fn write(&self, w: &mut IcsWriter) {
        w.prop("TZID", &self.tzid);

        for props in &self.daylights {
            w.comp("DAYLIGHT", props);
        }

        for props in &self.standards {
            w.comp("STANDARD", props);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VTimeZoneViolation {
    #[error("id `{0}` is already taken")]
    DuplicatedId(String),
}

/// Time zone properties; part of a [`VTimeZone`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsRead<Component> for TzProps {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut dtstart = None;
        let mut tz_offset_from = None;
        let mut tz_offset_to = None;
        let mut rrule = None;
        let mut tz_name = None;

        loop {
            let e = r.entry()?;

            if e.try_prop(r, "DTSTART", &mut dtstart)
                || e.try_prop(r, "TZOFFSETFROM", &mut tz_offset_from)
                || e.try_prop(r, "TZOFFSETTO", &mut tz_offset_to)
                || e.try_prop(r, "RRULE", &mut rrule)
                || e.try_prop(r, "TZNAME", &mut tz_name)
            {
                continue;
            }

            if e.try_comp_end("DAYLIGHT") || e.try_comp_end("STANDARD") {
                break;
            }

            e.burn(r, Kind::Component)?;
        }

        let dtstart = r.unwrap_prop("DTSTART", dtstart);
        let tz_offset_from = r.unwrap_prop("TZOFFSETFROM", tz_offset_from);
        let tz_offset_to = r.unwrap_prop("TZOFFSETTO", tz_offset_to);

        Some(Self {
            dtstart: dtstart?,
            tz_offset_from: tz_offset_from?,
            tz_offset_to: tz_offset_to?,
            rrule,
            tz_name,
        })
    }
}

impl IcsWrite<Component> for TzProps {
    fn write(&self, w: &mut IcsWriter) {
        w.prop("DTSTART", &self.dtstart);
        w.prop("TZOFFSETFROM", self.tz_offset_from);
        w.prop("TZOFFSETTO", self.tz_offset_to);
        w.prop_opt("RRULE", self.rrule.as_ref());
        w.prop_opt("TZNAME", self.tz_name.as_ref());
    }
}

/// Outcome of time zone parsing, see [`VTimeZone::from_str()`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedVTimeZone {
    pub tz: VTimeZone,

    /// Parsing messages (e.g. a syntax error somewhere in the file).
    pub msgs: Vec<ReadMsg>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Sign, UtcOffset, ics, utils::*};
    use pretty_assertions as pa;

    fn tz() -> VTimeZone {
        VTimeZone::new(
            "Alice/Wonderland",
            TzProps::new(
                dt("20180101T120000"),
                UtcOffset::new(Sign::Pos, 1, 0, 0).unwrap(),
                UtcOffset::new(Sign::Pos, 1, 30, 0).unwrap(),
            ),
            TzProps::new(
                dt("20180601T140000"),
                UtcOffset::new(Sign::Pos, 2, 30, 0).unwrap(),
                UtcOffset::new(Sign::Pos, 2, 0, 0).unwrap(),
            ),
        )
    }

    #[test]
    fn smoke() {
        let obj = tz();

        let str = ics! {"
            TZID:Alice/Wonderland
            BEGIN:DAYLIGHT
            DTSTART:20180101T120000
            TZOFFSETFROM:+0100
            TZOFFSETTO:+0130
            END:DAYLIGHT
            BEGIN:STANDARD
            DTSTART:20180601T140000
            TZOFFSETFROM:+0230
            TZOFFSETTO:+0200
            END:STANDARD
        "};

        pa::assert_eq!(str, obj.to_string(Component));
        assert_trip!(str, VTimeZone as Component("VTIMEZONE"));
    }

    #[test]
    fn viol_duplicated_id() {
        let cal = cal().with_timezone(tz());
        let actual = tz().validate(&cal, None);
        let expected = vec![VTimeZoneViolation::DuplicatedId("Alice/Wonderland".into())];

        assert_eq!(expected, actual);
    }
}
