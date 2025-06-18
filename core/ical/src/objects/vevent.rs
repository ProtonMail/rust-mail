use super::*;

/// Event; part of a [`VCalendar`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.6.1>
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct VEvent {
    pub uid: Option<Uid>,
    pub dtstamp: Option<DtStamp>,
    pub dtstart: Option<DtStart>,
    pub dtend: Option<DtEnd>,
    pub created: Option<Created>,

    pub class: Class,
    pub transp: Transp,
    pub status: Option<Status>,
    pub priority: Option<Priority>,

    pub summary: Option<Summary>,
    pub location: Option<Location>,
    pub description: Option<Description>,
    pub organizer: Option<Organizer>,
    pub attendees: Vec<Attendee>,

    pub rrule: Option<RRule>,
    pub exdate: Option<ExDate>,
    pub duration: Option<Duration>,
    pub sequence: Option<Sequence>,
    pub recurrence_id: Option<RecurrenceId>,

    pub alarms: Vec<VAlarm>,
}

impl VEvent {
    #[must_use]
    pub fn new(uid: impl Into<Uid>, dtstamp: impl Into<DtStamp>) -> Self {
        Self {
            uid: Some(uid.into()),
            dtstamp: Some(dtstamp.into()),
            dtstart: None,
            dtend: None,
            created: None,

            class: Class::default(),
            transp: Transp::default(),
            status: None,
            priority: None,

            summary: None,
            description: None,
            location: None,
            organizer: None,
            attendees: Vec::new(),

            rrule: None,
            exdate: None,
            duration: None,
            sequence: None,
            recurrence_id: None,

            alarms: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_dtstamp(mut self, dtstamp: impl Into<DtStamp>) -> Self {
        self.dtstamp = Some(dtstamp.into());
        self
    }

    #[must_use]
    pub fn with_dtstart(mut self, dtstart: impl Into<DtStart>) -> Self {
        self.dtstart = Some(dtstart.into());
        self
    }

    #[must_use]
    pub fn with_dtend(mut self, dtend: impl Into<DtEnd>) -> Self {
        self.dtend = Some(dtend.into());
        self
    }

    #[must_use]
    pub fn with_created(mut self, created: impl Into<Created>) -> Self {
        self.created = Some(created.into());
        self
    }

    #[must_use]
    pub fn with_class(mut self, class: impl Into<Class>) -> Self {
        self.class = class.into();
        self
    }

    #[must_use]
    pub fn with_transp(mut self, transp: impl Into<Transp>) -> Self {
        self.transp = transp.into();
        self
    }

    #[must_use]
    pub fn with_status(mut self, status: impl Into<Status>) -> Self {
        self.status = Some(status.into());
        self
    }

    #[must_use]
    pub fn with_priority(mut self, priority: impl Into<Priority>) -> Self {
        self.priority = Some(priority.into());
        self
    }

    #[must_use]
    pub fn with_summary(mut self, summary: impl Into<Summary>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    #[must_use]
    pub fn with_location(mut self, location: impl Into<Location>) -> Self {
        self.location = Some(location.into());
        self
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<Description>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn with_organizer(mut self, organizer: impl Into<Organizer>) -> Self {
        self.organizer = Some(organizer.into());
        self
    }

    #[must_use]
    pub fn with_attendee(mut self, attendee: impl Into<Attendee>) -> Self {
        self.attendees.push(attendee.into());
        self
    }

    #[must_use]
    pub fn with_attendees(mut self, attendees: impl IntoIterator<Item = Attendee>) -> Self {
        self.attendees.extend(attendees);
        self
    }

    #[must_use]
    pub fn with_rrule(mut self, rrule: impl Into<RRule>) -> Self {
        self.rrule = Some(rrule.into());
        self
    }

    #[must_use]
    pub fn with_exdate(mut self, exdate: ExDate) -> Self {
        self.exdate = Some(exdate);
        self
    }

    #[must_use]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    #[must_use]
    pub fn with_sequence(mut self, sequence: impl Into<Sequence>) -> Self {
        self.sequence = Some(sequence.into());
        self
    }

    #[must_use]
    pub fn with_recurrence_id(mut self, recurrence_id: impl Into<RecurrenceId>) -> Self {
        self.recurrence_id = Some(recurrence_id.into());
        self
    }

    #[must_use]
    pub fn with_alarm(mut self, alarm: impl Into<VAlarm>) -> Self {
        self.alarms.push(alarm.into());
        self
    }

    #[must_use]
    pub fn with_alarms(mut self, alarms: impl IntoIterator<Item = VAlarm>) -> Self {
        self.alarms.extend(alarms);
        self
    }

    /// Returns an iterator that goes over occurrences (aka instances,
    /// repetitions etc.) of this event.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use proton_ical::*;
    /// # use proton_ical::utils::*;
    /// #
    /// let event = VEvent::new("0001", dt("20180101T120000"))
    ///     .with_dtstart(d("20180101"))
    ///     .with_rrule(
    ///         Recur::new(Freq::Daily)
    ///             .with_by_day([ByDay::Every(Weekday::Monday)]),
    ///     );
    ///
    /// let mut dates = event.occurrences().unwrap();
    ///
    /// assert_eq!("2018-01-01T00:00:00+00:00[UTC]", dates.next().unwrap().to_string());
    /// assert_eq!("2018-01-08T00:00:00+00:00[UTC]", dates.next().unwrap().to_string());
    /// assert_eq!("2018-01-15T00:00:00+00:00[UTC]", dates.next().unwrap().to_string());
    /// ```
    ///
    /// If this event doesn't repeat, the iterator will emit just one date,
    /// `DTSTART`:
    ///
    /// ```rust
    /// # use proton_ical::*;
    /// # use proton_ical::utils::*;
    /// #
    /// let event = VEvent::new("0001", dt("20180101T120000"))
    ///     .with_dtstart(d("20180101"));
    ///
    /// let mut dates = event.occurrences().unwrap();
    ///
    /// assert_eq!("2018-01-01T00:00:00+00:00[UTC]", dates.next().unwrap().to_string());
    /// assert!(dates.next().is_none());
    /// ```
    ///
    /// If this event doesn't have `DTSTART`, this function will fail:
    ///
    /// ```rust
    /// # use proton_ical::*;
    /// # use proton_ical::utils::*;
    /// #
    /// let event = VEvent::new("0001", dt("20180101T120000"));
    ///
    /// assert!(event.occurrences().is_err());
    /// ```
    pub fn occurrences(&self) -> Result<RecurIterator, RecurIteratorError> {
        let dtstart = self
            .dtstart
            .as_ref()
            .ok_or(RecurIteratorError::MissingDtStart)?;

        let rrule = match &self.rrule {
            Some(rrule) => Cow::Borrowed(&rrule.value),

            None => {
                // If `RRULE` is missing, let's return an iterator that emits
                // just the `DTSTART` - the easiest way is by pretending an
                // implicit `COUNT=1` rule is actually present
                Cow::Owned(Recur::new(Freq::Daily).with_count(1))
            }
        };

        RecurIterator::new(&rrule, dtstart.value.clone())
    }

    #[must_use]
    pub(crate) fn validate(
        &self,
        cal: &VCalendar,
        skipping_idx: Option<usize>,
    ) -> Vec<VEventViolation> {
        let mut viols = Vec::new();

        if let Some(uid) = &self.uid {
            if cal
                .events
                .iter()
                .enumerate()
                .filter_map(|(idx, evt)| Some((idx, evt.uid.as_ref()?)))
                .any(|(idx, uid2)| Some(idx) != skipping_idx && uid2.value == uid.value)
            {
                viols.push(VEventViolation::DuplicatedUid(
                    uid.value.as_str().to_owned(),
                ));
            }
        } else {
            viols.push(VEventViolation::MissingUid);
        }

        if self.dtend.is_some() && self.duration.is_some() {
            viols.push(VEventViolation::BothDtEndAndDurationSpecified);
        }

        if let Some(dtstart) = &self.dtstart {
            let lhs = dtstart.value.ty();

            if let Some(dtend) = &self.dtend {
                let rhs = dtend.value.ty();

                if lhs != rhs {
                    viols.push(VEventViolation::DtStartAndDtEndTypeMismatch(lhs, rhs));
                }
            }

            if let Some(recurrence_id) = &self.recurrence_id {
                let rhs = recurrence_id.value.ty();

                if lhs != rhs {
                    viols.push(VEventViolation::DtStartAndRecurrenceIdTypeMismatch(
                        lhs, rhs,
                    ));
                }
            }
        }

        if let Some(dtstamp) = &self.dtstamp {
            viols.extend(dtstamp.validate(cal).into_iter().map_into());
        } else {
            viols.push(VEventViolation::MissingDtStamp);
        }

        if let Some(dtstart) = &self.dtstart {
            viols.extend(dtstart.validate(cal).into_iter().map_into());
        }

        if let Some(dtend) = &self.dtend {
            viols.extend(dtend.validate(cal).into_iter().map_into());
        }

        if let Some(created) = &self.created {
            viols.extend(created.validate(cal).into_iter().map_into());
        }

        if let Some(rrule) = &self.rrule {
            viols.extend(rrule.validate().into_iter().map_into());
        }

        if let Some(recurrence_id) = &self.recurrence_id {
            viols.extend(recurrence_id.validate(cal).into_iter().map_into());
        }

        for (idx, alarm) in self.alarms.iter().enumerate() {
            viols.extend(
                alarm
                    .validate(self)
                    .into_iter()
                    .map(|viol| VEventViolation::InvalidAlarm(idx, viol)),
            );
        }

        viols
    }
}

impl IcsRead<Component> for VEvent {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut uid = None;
        let mut dtstamp = None;
        let mut dtstart = None;
        let mut dtend = None;
        let mut created = None;

        let mut class = None;
        let mut transp = None;
        let mut status = None;
        let mut priority = None;

        let mut summary = None;
        let mut location = None;
        let mut description = None;
        let mut organizer = None;
        let mut attendees = Vec::new();

        let mut rrule = None;
        let mut exdate = None;
        let mut duration = None;
        let mut sequence = None;
        let mut recurrence_id = None;

        let mut alarms = Vec::new();

        loop {
            let e = r.entry()?;

            if e.try_prop(r, "UID", &mut uid)
                || e.try_prop(r, "DTSTAMP", &mut dtstamp)
                || e.try_prop(r, "DTSTART", &mut dtstart)
                || e.try_prop(r, "DTEND", &mut dtend)
                || e.try_prop(r, "CREATED", &mut created)
                // ---
                || e.try_prop(r, "CLASS", &mut class)
                || e.try_prop(r, "TRANSP", &mut transp)
                || e.try_prop(r, "STATUS", &mut status)
                || e.try_prop(r, "PRIORITY", &mut priority)
                // ---
                || e.try_prop(r, "SUMMARY", &mut summary)
                || e.try_prop(r, "LOCATION", &mut location)
                || e.try_prop(r, "DESCRIPTION", &mut description)
                || e.try_prop(r, "ORGANIZER", &mut organizer)
                || e.try_props(r, "ATTENDEE", &mut attendees)
                // ---
                || e.try_prop(r, "RRULE", &mut rrule)
                || e.try_prop(r, "EXDATE", &mut exdate)
                || e.try_prop(r, "DURATION", &mut duration)
                || e.try_prop(r, "SEQUENCE", &mut sequence)
                || e.try_prop(r, "RECURRENCE-ID", &mut recurrence_id)
                // ---
                || e.try_comps(r, "VALARM", &mut alarms)
            {
                continue;
            }

            if e.try_comp_end("VEVENT") {
                break;
            }

            e.burn(r, Kind::Component)?;
        }

        let uid = r.unwrap_prop("UID", uid);
        let dtstamp = r.unwrap_prop("DTSTAMP", dtstamp);

        Some(Self {
            uid,
            dtstamp,
            dtstart,
            dtend,
            created,

            class: class.unwrap_or_default(),
            transp: transp.unwrap_or_default(),
            status,
            priority,

            summary,
            location,
            description,

            organizer,
            attendees,

            rrule,
            exdate,
            duration,
            sequence,
            recurrence_id,

            alarms,
        })
    }
}

impl IcsWrite<Component> for VEvent {
    fn write(&self, w: &mut IcsWriter) {
        w.prop_opt("UID", self.uid.as_ref());
        w.prop_opt("DTSTAMP", self.dtstamp.as_ref());
        w.prop_opt("DTSTART", self.dtstart.as_ref());
        w.prop_opt("DTEND", self.dtend.as_ref());
        w.prop_opt("CREATED", self.created.as_ref());

        if self.class == Class::default() {
            // Implementations tend to omit the default `CLASS:PUBLIC`
        } else {
            w.prop("CLASS", self.class);
        }

        if self.transp == Transp::default() {
            // Implementations tend to omit the default `TRANSP:OPAQUE`
        } else {
            w.prop("TRANSP", self.transp);
        }

        w.prop_opt("STATUS", self.status);
        w.prop_opt("PRIORITY", self.priority);
        w.prop_opt("SUMMARY", self.summary.as_ref());
        w.prop_opt("LOCATION", self.location.as_ref());
        w.prop_opt("DESCRIPTION", self.description.as_ref());
        w.prop_opt("ORGANIZER", self.organizer.as_ref());

        for attendee in &self.attendees {
            w.prop("ATTENDEE", attendee);
        }

        w.prop_opt("RRULE", self.rrule.as_ref());
        w.prop_opt("EXDATE", self.exdate.as_ref());
        w.prop_opt("DURATION", self.duration.as_ref());
        w.prop_opt("SEQUENCE", self.sequence.as_ref());
        w.prop_opt("RECURRENCE-ID", self.recurrence_id.as_ref());

        for alarm in &self.alarms {
            w.comp("VALARM", alarm);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VEventViolation {
    #[error("uid `{0}` is already taken")]
    DuplicatedUid(String),

    #[error("uid is missing")]
    MissingUid,

    #[error("dtend is exclusive with duration")]
    BothDtEndAndDurationSpecified,

    #[error("dtstart ({0}) has different type than dtend ({0})")]
    DtStartAndDtEndTypeMismatch(&'static str, &'static str),

    #[error("dtstart ({0}) has different type than recurrence-id ({0})")]
    DtStartAndRecurrenceIdTypeMismatch(&'static str, &'static str),

    #[error("dtstamp is missing")]
    MissingDtStamp,

    #[error("dtstamp: {0}")]
    InvalidDtStamp(#[from] DtStampViolation),

    #[error("dtstart: {0}")]
    InvalidDtStart(#[from] DtStartViolation),

    #[error("dtend: {0}")]
    InvalidDtEnd(#[from] DtEndViolation),

    #[error("created: {0}")]
    InvalidCreated(#[from] CreatedViolation),

    #[error("rrule: {0}")]
    InvalidRRule(#[from] RRuleViolation),

    #[error("recurrence-id: {0}")]
    InvalidRecurrenceId(#[from] RecurrenceIdViolation),

    #[error("alarm[{0}]: {1}")]
    InvalidAlarm(usize, VAlarmViolation),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CuType, DateTimeViolation, DisplayAlarm, EmailAlarm, Trigger, ics, utils::*};
    use pretty_assertions as pa;

    fn event() -> VEvent {
        VEvent::new("1234", dt("20180101T120000Z"))
    }

    #[track_caller]
    fn assert(obj: &VEvent, str: &str) {
        pa::assert_eq!(str, obj.to_string(Component));
        assert_trip!(str, VEvent as Component("VEVENT"));
    }

    #[test]
    fn smoke() {
        let obj = event();

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_dtstart_as_date() {
        let obj = event().with_dtstart(d("20180101"));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            DTSTART;VALUE=DATE:20180101
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_dtstart_as_datetime() {
        let obj = event().with_dtstart(dt("20180101T120000"));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            DTSTART:20180101T120000
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_dtend() {
        let obj = event().with_dtend(d("20180101"));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            DTEND;VALUE=DATE:20180101
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_created() {
        let obj = event().with_created(dt("20180101T120000"));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            CREATED:20180101T120000
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_class() {
        let obj = event().with_class(Class::Private);

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            CLASS:PRIVATE
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_transp() {
        let obj = event().with_transp(Transp::Transparent);

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            TRANSP:TRANSPARENT
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_status() {
        let obj = event().with_status(Status::Confirmed);

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            STATUS:CONFIRMED
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_priority() {
        let obj = event().with_priority(7);

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            PRIORITY:7
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_summary() {
        let obj = event().with_summary("couldn't this have been an email?!");

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            SUMMARY:couldn't this have been an email?!
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_location() {
        let obj = event().with_location("Online");

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            LOCATION:Online
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_description() {
        let obj = event().with_description("Very Important Meeting");

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            DESCRIPTION:Very Important Meeting
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_organizer() {
        let obj = event().with_organizer(email("saul@goodman"));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            ORGANIZER:mailto:saul@goodman
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_attendee() {
        let obj = event().with_attendee(
            Attendee::from(email("someone@localhost")).with_cutype(CuType::Individual),
        );

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            ATTENDEE;CUTYPE=INDIVIDUAL:mailto:someone@localhost
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_rrule() {
        let obj = event().with_rrule(Recur::new(Freq::Daily).with_count(5));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            RRULE:FREQ=DAILY;COUNT=5
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_exdate() {
        let obj = event().with_exdate(ExDate::Dates(vec![
            d("20180101"),
            d("20180102"),
            d("20180103"),
        ]));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            EXDATE;VALUE=DATE:20180101,20180102,20180103
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_duration() {
        let obj = event().with_duration(dur("P2D"));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            DURATION:P2D
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_sequence() {
        let obj = event().with_sequence(123);

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            SEQUENCE:123
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_recurrence_id() {
        let obj = event()
            .with_dtstart(d("20180101"))
            .with_recurrence_id(d("20180102"));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            DTSTART;VALUE=DATE:20180101
            RECURRENCE-ID;VALUE=DATE:20180102
        "};

        assert(&obj, &str);
    }

    #[test]
    fn with_alarm() {
        let obj = event().with_alarm(EmailAlarm::new(
            Trigger::start(dur("P2D")),
            "some description",
            "some summary",
            email("someone@localhost"),
        ));

        let str = ics! {"
            UID:1234
            DTSTAMP:20180101T120000Z
            BEGIN:VALARM
            ACTION:EMAIL
            TRIGGER:P2D
            DESCRIPTION:some description
            SUMMARY:some summary
            ATTENDEE:mailto:someone@localhost
            END:VALARM
        "};

        assert(&obj, &str);
    }

    #[test]
    fn viol_duplicated_uid() {
        let mut target = cal();

        for id in ["1", "2", "3"] {
            target.events.push(VEvent::new(id, dt("20180101T120000Z")));
        }

        let actual = VEvent::new("2", dt("20180101T120000Z")).validate(&target, None);
        let expected = vec![VEventViolation::DuplicatedUid("2".into())];

        assert_eq!(expected, actual);
    }

    #[test]
    fn viol_missing_uid() {
        let target = VEvent {
            uid: None,
            ..event()
        };

        let expected = vec![VEventViolation::MissingUid];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_both_dtend_and_duration_specified() {
        let target = event()
            .with_dtend(dt("20180101T120000Z"))
            .with_duration(dur("P1D"));

        let expected = vec![VEventViolation::BothDtEndAndDurationSpecified];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_dtstart_and_dtend_type_mismatch() {
        let target = event()
            .with_dtstart(d("20180101"))
            .with_dtend(dt("20180101T120000Z"));

        let expected = vec![VEventViolation::DtStartAndDtEndTypeMismatch(
            "date",
            "date-time",
        )];

        assert_eq!(expected, target.validate(&cal(), None));

        // ---

        let target = event()
            .with_dtstart(d("20180101"))
            .with_dtend(d("20180101"));

        assert!(target.validate(&cal(), None).is_empty());
    }

    #[test]
    fn viol_dtstart_and_recurrence_id_type_mismatch() {
        let target = event()
            .with_dtstart(d("20180101"))
            .with_recurrence_id(dt("20180101T120000Z"));

        let expected = vec![VEventViolation::DtStartAndRecurrenceIdTypeMismatch(
            "date",
            "date-time",
        )];

        assert_eq!(expected, target.validate(&cal(), None));

        // ---

        let target = event()
            .with_dtstart(d("20180101"))
            .with_recurrence_id(d("20180101"));

        assert!(target.validate(&cal(), None).is_empty());
    }

    #[test]
    fn viol_missing_dtstamp() {
        let target = VEvent {
            dtstamp: None,
            ..event()
        };

        let expected = vec![VEventViolation::MissingDtStamp];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_invalid_dtstamp() {
        let target = VEvent::new("1", dt(";TZID=Europe/Warsaw:20180101T120000"));

        let expected = vec![VEventViolation::InvalidDtStamp(
            DtStampViolation::InvalidValue(DateTimeViolation::UnknownTimeZone(
                "Europe/Warsaw".into(),
            )),
        )];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_invalid_dtstart() {
        let target = event().with_dtstart(dt(";TZID=Europe/Warsaw:20180101T120000"));

        let expected = vec![VEventViolation::InvalidDtStart(
            DtStartViolation::InvalidValue(DateTimeViolation::UnknownTimeZone(
                "Europe/Warsaw".into(),
            )),
        )];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_invalid_created() {
        let target = event().with_created(dt(";TZID=Europe/Warsaw:20180101T120000"));

        let expected = vec![VEventViolation::InvalidCreated(
            CreatedViolation::InvalidValue(DateTimeViolation::UnknownTimeZone(
                "Europe/Warsaw".into(),
            )),
        )];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_invalid_rrule() {
        let target = event().with_rrule(Recur::new(Freq::Daily).with_interval(0));

        let expected = vec![VEventViolation::InvalidRRule(RRuleViolation::InvalidValue(
            RecurViolation::ZeroInterval,
        ))];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_invalid_recurrence_id() {
        let target = event().with_recurrence_id(dt(";TZID=Europe/Warsaw:20180101T120000"));

        let expected = vec![VEventViolation::InvalidRecurrenceId(
            RecurrenceIdViolation::InvalidValue(DateTimeViolation::UnknownTimeZone(
                "Europe/Warsaw".into(),
            )),
        )];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_invalid_dtend() {
        let target = event().with_dtend(dt(";TZID=Europe/Warsaw:20180101T120000"));

        let expected = vec![VEventViolation::InvalidDtEnd(DtEndViolation::InvalidValue(
            DateTimeViolation::UnknownTimeZone("Europe/Warsaw".into()),
        ))];

        assert_eq!(expected, target.validate(&cal(), None));
    }

    #[test]
    fn viol_invalid_alarm() {
        let target = event().with_alarm(DisplayAlarm::new(
            Trigger::start(dur("-PT15M")),
            "some description",
        ));

        let expected = vec![VEventViolation::InvalidAlarm(
            0,
            VAlarmViolation::MissingStartDay,
        )];

        assert_eq!(expected, target.validate(&cal(), None));

        // ---

        let target = target.with_dtstart(d("20180101"));

        assert!(target.validate(&cal(), None).is_empty());
    }
}
