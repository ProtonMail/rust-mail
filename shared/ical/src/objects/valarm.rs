use super::*;

/// Alarm; part of a [`VEvent`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.6.6>
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VAlarm {
    Display(DisplayAlarm),
    Email(EmailAlarm),
}

impl VAlarm {
    #[must_use]
    pub fn trigger(&self) -> Trigger {
        match self {
            VAlarm::Display(this) => this.trigger,
            VAlarm::Email(this) => this.trigger,
        }
    }

    #[must_use]
    pub fn description(&self) -> &Description {
        match self {
            VAlarm::Display(this) => &this.description,
            VAlarm::Email(this) => &this.description,
        }
    }

    #[must_use]
    pub(crate) fn validate(&self, evt: &VEvent) -> Vec<VAlarmViolation> {
        let mut viols = Vec::new();

        match self.trigger() {
            Trigger::Relative(TriggerEdge::Start, _) => {
                if evt.dtstart.is_none() {
                    viols.push(VAlarmViolation::MissingStartDay);
                }
            }

            Trigger::Relative(TriggerEdge::End, _) => {
                if evt.dtend.is_none() && evt.dtstart.is_none() && evt.duration.is_none() {
                    viols.push(VAlarmViolation::MissingEndDay);
                }
            }

            Trigger::Absolute(_) => (),
        }

        if let VAlarm::Email(this) = self {
            viols.extend(this.validate());
        }

        viols
    }
}

impl From<DisplayAlarm> for VAlarm {
    fn from(value: DisplayAlarm) -> Self {
        VAlarm::Display(value)
    }
}

impl From<EmailAlarm> for VAlarm {
    fn from(value: EmailAlarm) -> Self {
        VAlarm::Email(value)
    }
}

impl IcsRead<Component> for VAlarm {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut action = None;
        let mut trigger = None;
        let mut description = None;
        let mut duration = None;
        let mut repeat = None;
        let mut summary = None;
        let mut attendees = Vec::new();

        loop {
            let e = r.entry()?;

            if e.try_prop(r, "ACTION", &mut action)
                || e.try_prop(r, "TRIGGER", &mut trigger)
                || e.try_prop(r, "DESCRIPTION", &mut description)
                || e.try_prop(r, "DURATION", &mut duration)
                || e.try_prop(r, "REPEAT", &mut repeat)
                || e.try_prop(r, "SUMMARY", &mut summary)
                || e.try_props(r, "ATTENDEE", &mut attendees)
            {
                continue;
            }

            if e.try_comp_end("VALARM") {
                break;
            }

            e.burn(r, Kind::Component)?;
        }

        let duration_and_repeat = if duration.is_some() || repeat.is_some() {
            DurationAndRepeat::new_opt(
                r.unwrap_prop("DURATION", duration),
                r.unwrap_prop("REPEAT", repeat),
            )
        } else {
            None
        };

        match r.unwrap_prop("ACTION", action)? {
            VAlarmAction::Display => {
                let trigger = r.unwrap_prop("TRIGGER", trigger);
                let description = r.unwrap_prop("DESCRIPTION", description);

                Some(VAlarm::Display(DisplayAlarm {
                    trigger: trigger?,
                    description: description?,
                    duration_and_repeat,
                }))
            }

            VAlarmAction::Email => {
                let trigger = r.unwrap_prop("TRIGGER", trigger);
                let description = r.unwrap_prop("DESCRIPTION", description);
                let summary = r.unwrap_prop("SUMMARY", summary);

                Some(VAlarm::Email(EmailAlarm {
                    trigger: trigger?,
                    description: description?,
                    summary: summary?,
                    attendees,
                    duration_and_repeat,
                }))
            }
        }
    }
}

impl IcsWrite<Component> for VAlarm {
    fn write(&self, w: &mut IcsWriter) {
        match self {
            VAlarm::Display(this) => {
                w.prop("ACTION", VAlarmAction::Display);
                this.write(w);
            }
            VAlarm::Email(this) => {
                w.prop("ACTION", VAlarmAction::Email);
                this.write(w);
            }
        }
    }
}

/// Alarm's type; see [`VAlarm`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlarmAction {
    Display,
    Email,
}

impl IcsRead<Property> for VAlarmAction {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("DISPLAY") {
            Some(VAlarmAction::Display)
        } else if value.eq_ignore_ascii_case("EMAIL") {
            Some(VAlarmAction::Email)
        } else {
            r.error(span, format!("unknown alarm action `{value}`"));
            None
        }
    }
}

impl IcsWrite<Property> for VAlarmAction {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");

        w.raw(match self {
            VAlarmAction::Display => "DISPLAY",
            VAlarmAction::Email => "EMAIL",
        });
    }
}

/// Alarm that displays a message; see [`VAlarm`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct DisplayAlarm {
    pub trigger: Trigger,
    pub description: Description,
    pub duration_and_repeat: Option<DurationAndRepeat>,
}

impl DisplayAlarm {
    #[must_use]
    pub fn new(trigger: Trigger, description: impl Into<Description>) -> Self {
        Self {
            trigger,
            description: description.into(),
            duration_and_repeat: None,
        }
    }

    #[must_use]
    pub fn with_duration_and_repeat(
        mut self,
        duration: Duration,
        repeat: impl Into<Repeat>,
    ) -> Self {
        self.duration_and_repeat = Some(DurationAndRepeat {
            duration,
            repeat: repeat.into(),
        });

        self
    }
}

impl IcsWrite<Component> for DisplayAlarm {
    fn write(&self, w: &mut IcsWriter) {
        w.prop("TRIGGER", self.trigger);
        w.prop("DESCRIPTION", &self.description);

        if let Some(dr) = self.duration_and_repeat {
            w.prop("DURATION", dr.duration);
            w.prop("REPEAT", dr.repeat);
        }
    }
}

/// Alarm that sends an email; see [`VAlarm`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct EmailAlarm {
    pub trigger: Trigger,
    pub description: Description,
    pub summary: Summary,
    pub attendees: Vec<EmailAddress>,
    pub duration_and_repeat: Option<DurationAndRepeat>,
}

impl EmailAlarm {
    #[must_use]
    pub fn new(
        trigger: Trigger,
        description: impl Into<Description>,
        summary: impl Into<Summary>,
        attendee: EmailAddress,
        // ^ explicitly not `impl Into<...>` to highlight it needs to be an
        //   e-mail address
    ) -> Self {
        Self {
            trigger,
            description: description.into(),
            summary: summary.into(),
            attendees: vec![attendee],
            duration_and_repeat: None,
        }
    }

    #[must_use]
    pub fn with_attendee(mut self, attendee: EmailAddress) -> Self {
        self.attendees.push(attendee);
        self
    }

    #[must_use]
    pub fn with_duration_and_repeat(
        mut self,
        duration: Duration,
        repeat: impl Into<Repeat>,
    ) -> Self {
        self.duration_and_repeat = Some(DurationAndRepeat {
            duration,
            repeat: repeat.into(),
        });

        self
    }

    pub(crate) fn validate(&self) -> Vec<VAlarmViolation> {
        let mut viols = Vec::new();

        if self.attendees.is_empty() {
            viols.push(VAlarmViolation::NoAttendees);
        }

        viols
    }
}

impl IcsWrite<Component> for EmailAlarm {
    fn write(&self, w: &mut IcsWriter) {
        w.prop("TRIGGER", self.trigger);
        w.prop("DESCRIPTION", &self.description);
        w.prop("SUMMARY", &self.summary);

        for attendee in &self.attendees {
            w.prop("ATTENDEE", attendee);
        }

        if let Some(dr) = self.duration_and_repeat {
            w.prop("DURATION", dr.duration);
            w.prop("REPEAT", dr.repeat);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct DurationAndRepeat {
    pub duration: Duration,
    pub repeat: Repeat,
}

impl DurationAndRepeat {
    fn new_opt(duration: Option<Duration>, repeat: Option<Repeat>) -> Option<Self> {
        Some(Self {
            duration: duration?,
            repeat: repeat?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VAlarmViolation {
    #[error("alarm's parent has no start day (dtstart)")]
    MissingStartDay,

    #[error("alarm's parent has no end day (dtend or dtstart+duration)")]
    MissingEndDay,

    #[error("alarm has no attendees")]
    NoAttendees,
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for VAlarm {
        const TYPE: PhpDataType = PhpDataType::Object(None);

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            match zval.object()?.get_property("kind").ok()? {
                "Display" => Some(VAlarm::Display(DisplayAlarm::from_zval(zval)?)),
                "Email" => Some(VAlarm::Email(EmailAlarm::from_zval(zval)?)),
                _ => None,
            }
        }
    }

    impl IntoPhpZval for VAlarm {
        const TYPE: PhpDataType = PhpDataType::Object(None);
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            let kind = match self {
                VAlarm::Display(this) => {
                    this.set_zval(zval, persistent)?;
                    "Display"
                }

                VAlarm::Email(this) => {
                    this.set_zval(zval, persistent)?;
                    "Email"
                }
            };

            zval.object_mut().unwrap().set_property("kind", kind)?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ics;
    use crate::utils::*;
    use pretty_assertions as pa;

    fn display_target() -> DisplayAlarm {
        DisplayAlarm::new(Trigger::start(dur("-PT1H")), "some description")
    }

    fn email_target() -> EmailAlarm {
        EmailAlarm::new(
            Trigger::start(dur("-PT1H")),
            "some description",
            "some summary",
            email("someone@localhost"),
        )
    }

    fn event() -> VEvent {
        VEvent::new("1", dt("20180101T120000Z"))
    }

    #[track_caller]
    fn assert(obj: &VAlarm, str: &str) {
        pa::assert_eq!(str, obj.to_string(Component));
        assert_trip!(str, VAlarm as Component("VALARM"));
    }

    #[test]
    fn display_alarm() {
        let obj = VAlarm::Display(display_target());

        let str = ics! {"
            ACTION:DISPLAY
            TRIGGER:-PT1H
            DESCRIPTION:some description
        "};

        assert(&obj, &str);
    }

    #[test]
    fn repeating_display_alarm() {
        let obj = VAlarm::Display(display_target().with_duration_and_repeat(dur("PT15M"), 10));

        let str = ics! {"
            ACTION:DISPLAY
            TRIGGER:-PT1H
            DESCRIPTION:some description
            DURATION:PT15M
            REPEAT:10
        "};

        assert(&obj, &str);
    }

    #[test]
    fn email_alarm() {
        let obj = VAlarm::Email(email_target());

        let str = ics! {"
            ACTION:EMAIL
            TRIGGER:-PT1H
            DESCRIPTION:some description
            SUMMARY:some summary
            ATTENDEE:mailto:someone@localhost
        "};

        assert(&obj, &str);
    }

    #[test]
    fn repeating_email_alarm() {
        let obj = VAlarm::Email(email_target().with_duration_and_repeat(dur("PT15M"), 10));

        let str = ics! {"
            ACTION:EMAIL
            TRIGGER:-PT1H
            DESCRIPTION:some description
            SUMMARY:some summary
            ATTENDEE:mailto:someone@localhost
            DURATION:PT15M
            REPEAT:10
        "};

        assert(&obj, &str);
    }

    #[test]
    fn viol_missing_start_day() {
        let evt = event();

        let target = VAlarm::Display(DisplayAlarm::new(
            Trigger::start(dur("-PT15M")),
            "some description",
        ));

        let expected = vec![VAlarmViolation::MissingStartDay];

        assert_eq!(expected, target.validate(&evt));

        // ---
        // Make sure that validation passes when event has a start day.

        let evt = event().with_dtstart(dt("20180101T120000Z"));

        let target = VAlarm::Display(DisplayAlarm::new(
            Trigger::start(dur("-PT15M")),
            "some description",
        ));

        assert!(target.validate(&evt).is_empty());

        // ---
        // Make sure that validation passes when alarm has an absolute trigger.

        let evt = event();

        let target = VAlarm::Display(DisplayAlarm::new(
            Trigger::abs(dte("20180101T120000Z")),
            "some description",
        ));

        assert!(target.validate(&evt).is_empty());
    }

    #[test]
    fn viol_missing_end_day() {
        let evt = event();

        let target = VAlarm::Display(DisplayAlarm::new(
            Trigger::end(dur("-PT15M")),
            "some description",
        ));

        let expected = vec![VAlarmViolation::MissingEndDay];

        assert_eq!(expected, target.validate(&evt));

        // ---
        // Make sure that validation passes when event has an end day.

        let evt = event().with_dtend(dt("20180101T120000Z"));

        let target = VAlarm::Display(DisplayAlarm::new(
            Trigger::end(dur("-PT15M")),
            "some description",
        ));

        assert!(target.validate(&evt).is_empty());

        // ---
        // Make sure that validation passes when event has start day and
        // duration.

        let evt = event()
            .with_dtstart(dt("20180101T120000Z"))
            .with_duration(dur("PT1H"));

        let target = VAlarm::Display(DisplayAlarm::new(
            Trigger::end(dur("-PT15M")),
            "some description",
        ));

        assert!(target.validate(&evt).is_empty());

        // ---
        // Make sure that validation passes when alarm has an absolute trigger.

        let evt = event();

        let target = VAlarm::Display(DisplayAlarm::new(
            Trigger::abs(dte("20180101T120000Z")),
            "some description",
        ));

        assert!(target.validate(&evt).is_empty());
    }

    #[test]
    fn viol_no_attendees() {
        let evt = event().with_dtstart(d("20180101"));
        let mut target = email_target();

        assert!(VAlarm::Email(target.clone()).validate(&evt).is_empty());

        // ---

        target.attendees.clear();

        // ---

        let actual = VAlarm::Email(target).validate(&evt);
        let expected = vec![VAlarmViolation::NoAttendees];

        assert_eq!(expected, actual);
    }
}
