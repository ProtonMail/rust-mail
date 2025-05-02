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

/// Alarm's type; see [`VAlarm`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlarmAction {
    Display,
    Email,
}

/// Alarm that displays a message; see [`VAlarm`].
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Alarm that sends an email; see [`VAlarm`].
#[derive(Clone, Debug, PartialEq, Eq)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DurationAndRepeat {
    pub duration: Duration,
    pub repeat: Repeat,
}
