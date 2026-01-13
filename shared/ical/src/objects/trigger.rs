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

impl IcsRead<Property> for Trigger {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut related: Option<Spanned<ParamValue>> = None;
        let mut value: Option<Spanned<ParamValue>> = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "RELATED", &mut related) || e.try_param(r, "VALUE", &mut value) {
                continue;
            }

            if e.is_value() {
                break;
            }

            e.burn(r, Kind::Property)?;
        }

        if let Some(Spanned { span, value }) = value {
            let value = value.as_str();

            return if value.eq_ignore_ascii_case("DATE-TIME") {
                Some(Trigger::Absolute(r.value()?))
            } else {
                r.error(span, format!("unknown trigger value `{value}`"));
                None
            };
        }

        if let Some(Spanned { span, value }) = related {
            let value = value.as_str();

            return if value.eq_ignore_ascii_case("START") {
                Some(Trigger::Relative(TriggerEdge::Start, r.value()?))
            } else if value.eq_ignore_ascii_case("END") {
                Some(Trigger::Relative(TriggerEdge::End, r.value()?))
            } else {
                r.error(span, format!("unknown trigger relation `{value}`"));
                None
            };
        }

        Some(Trigger::Relative(TriggerEdge::Start, r.value()?))
    }
}

impl IcsWrite<Property> for Trigger {
    fn write(&self, w: &mut IcsWriter) {
        match self {
            Trigger::Relative(TriggerEdge::Start, _) => {
                // Implied `RELATED=START`
            }
            Trigger::Relative(TriggerEdge::End, _) => {
                w.param("RELATED", TextRef::new_unchecked("END"));
            }
            Trigger::Absolute(_) => {
                w.param("VALUE", TextRef::new_unchecked("DATE-TIME"));
            }
        }

        w.raw(":");

        match self {
            Trigger::Relative(_, this) => w.value(this),
            Trigger::Absolute(this) => w.value(this),
        }
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

#[cfg(feature = "php")]
mod php {
    use super::*;

    #[derive(Clone, Debug, ZvalConvert)]
    enum PhpTrigger {
        Absolute(PhpAbsoluteTrigger),
        Relative(PhpRelativeTrigger),
    }

    #[derive(Clone, Debug, ZvalConvert)]
    struct PhpRelativeTrigger {
        kind: String,
        edge: TriggerEdge,
        duration: Duration,
    }

    #[derive(Clone, Debug, ZvalConvert)]
    struct PhpAbsoluteTrigger {
        kind: String,
        at: DateTime<UtcForm>,
    }

    impl From<Trigger> for PhpTrigger {
        fn from(value: Trigger) -> Self {
            match value {
                Trigger::Absolute(at) => PhpTrigger::Absolute(PhpAbsoluteTrigger {
                    kind: "Absolute".into(),
                    at,
                }),

                Trigger::Relative(edge, duration) => PhpTrigger::Relative(PhpRelativeTrigger {
                    kind: "Relative".into(),
                    edge,
                    duration,
                }),
            }
        }
    }

    impl From<PhpTrigger> for Trigger {
        fn from(value: PhpTrigger) -> Self {
            match value {
                PhpTrigger::Absolute(value) => Trigger::Absolute(value.at),
                PhpTrigger::Relative(value) => Trigger::Relative(value.edge, value.duration),
            }
        }
    }

    impl<'a> FromPhpZval<'a> for Trigger {
        const TYPE: PhpDataType = <PhpTrigger as FromPhpZval<'a>>::TYPE;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            Some(PhpTrigger::from_zval(zval)?.into())
        }
    }

    impl IntoPhpZval for Trigger {
        const TYPE: PhpDataType = <PhpTrigger as IntoPhpZval>::TYPE;
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            PhpTrigger::from(self).set_zval(zval, persistent)
        }
    }

    impl<'a> FromPhpZval<'a> for TriggerEdge {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            match zval.str()? {
                "Start" => Some(TriggerEdge::Start),
                "End" => Some(TriggerEdge::End),
                _ => None,
            }
        }
    }

    impl IntoPhpZval for TriggerEdge {
        const TYPE: PhpDataType = PhpDataType::String;
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            zval.set_string(&format!("{self:?}"), persistent)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;
    use test_case::test_case;

    #[test_case(
        Trigger::start(dur("PT30M")), ":PT30M"
        ; "relative-start with positive duration"
    )]
    #[test_case(
        Trigger::start(dur("-PT30M")), ":-PT30M"
        ; "relative-start with negative duration"
    )]
    #[test_case(
        Trigger::end(dur("PT30M")), ";RELATED=END:PT30M"
        ; "relative-end with positive duration"
    )]
    #[test_case(
        Trigger::end(dur("-PT30M")), ";RELATED=END:-PT30M"
        ; "relative-end with negative duration"
    )]
    #[test_case(
        Trigger::abs(dte("20180101T120000Z")), ";VALUE=DATE-TIME:20180101T120000Z"
        ; "absolute"
    )]
    fn smoke(obj: Trigger, str: &str) {
        assert_eq!(obj, Trigger::from_str(str, Property).unwrap());
        assert_trip!(str, Trigger as Property);
    }
}
