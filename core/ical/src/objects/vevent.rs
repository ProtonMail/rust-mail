use super::*;

/// Event; part of a [`VCalendar`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.6.1>
#[derive(Clone, Debug, PartialEq, Eq)]
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
}
