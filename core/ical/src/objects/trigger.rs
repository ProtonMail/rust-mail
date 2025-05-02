use super::*;

/// Trigger.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.6.3>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Absolute(DateTime<UtcForm>),
    Relative(TriggerEdge, Duration),
}

impl Trigger {
    /// Creates a trigger that fires at given specific time.
    #[must_use]
    pub fn abs(at: DateTime<UtcForm>) -> Self {
        Trigger::Absolute(at)
    }

    /// Creates a trigger that fires at given duration before or after the event
    /// has started.
    ///
    /// - if given duration is negative, the trigger will fire before the event
    ///   has started,
    ///
    /// - if given duration is positive, the trigger will fire after the event
    ///   has started.
    #[must_use]
    pub fn start(dur: Duration) -> Self {
        Trigger::Relative(TriggerEdge::Start, dur)
    }

    /// Creates a trigger that fires at given duration before or after the event
    /// has finished.
    ///
    /// - if given duration is negative, the trigger will fire before the event
    ///   has finished,
    ///
    /// - if given duration is positive, the trigger will fire after the event
    ///   has finished.
    #[must_use]
    pub fn end(dur: Duration) -> Self {
        Trigger::Relative(TriggerEdge::End, dur)
    }
}

/// Trigger's edge, part of a [`Trigger`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.6.3>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerEdge {
    Start,
    End,
}
