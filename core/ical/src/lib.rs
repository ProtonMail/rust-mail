#![allow(
    clippy::wildcard_imports,
    reason = "it's just waay more convenient this way - especially around objects such as VEvent which import half of the crate"
)]

mod io;
mod objects;
mod result;
pub mod utils;

pub(crate) use self::io::*;
pub use self::objects::*;
pub use self::result::*;

/// Calendar, as described in RFC5545.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.4>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ext_php_rs::ZvalConvert))]
pub struct VCalendar {
    method: Option<Method>,
    prodid: ProdId,
    version: Version,
    calscale: CalScale,
    events: Vec<VEvent>,
    timezones: Vec<VTimeZone>,
}

impl VCalendar {
    #[must_use]
    pub fn new(prodid: impl Into<ProdId>) -> Self {
        Self {
            method: None,
            prodid: prodid.into(),
            version: Version::Two,
            calscale: CalScale::Gregorian,
            events: Vec::new(),
            timezones: Vec::new(),
        }
    }

    #[must_use]
    pub fn method(&self) -> Option<Method> {
        self.method
    }

    pub fn set_method(&mut self, method: Option<Method>) {
        self.method = method;
    }

    #[must_use]
    pub fn with_method(mut self, method: Method) -> Self {
        self.set_method(Some(method));
        self
    }

    #[must_use]
    pub fn prodid(&self) -> &ProdId {
        &self.prodid
    }

    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    #[must_use]
    pub fn calscale(&self) -> CalScale {
        self.calscale
    }

    #[must_use]
    pub fn events(&self) -> &[VEvent] {
        &self.events
    }

    /// Adds an event into the calendar.
    ///
    /// Event is validated before the insert and if any violation occurs (event
    /// refers to an unknown time zone, the calendar already contains event with
    /// this id etc.), an error is returned and the event is not inserted.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ical::utils::*;
    /// use ical::{VCalendar, VEvent, ical};
    ///
    /// let mut cal = VCalendar::new("test");
    ///
    /// cal.add_event(VEvent::new("1", dt("20180101T120000Z")))
    ///    .unwrap();
    ///
    /// assert_eq!(1, cal.events().len());
    /// ```
    pub fn add_event(&mut self, event: VEvent) -> Result<()> {
        event
            .validate(self, None)
            .into_result(|viol| Violation::InvalidEvent(self.events.len(), viol))?;

        self.events.push(event);

        Ok(())
    }

    /// Adds an event into the calendar; see [`Self::add_event()`].
    pub fn with_event(mut self, event: VEvent) -> Result<Self> {
        self.add_event(event)?;

        Ok(self)
    }

    /// Modifies an event in the calendar.
    ///
    /// Event is revalidated before replacing the existing one and if any
    /// violation occurs, an error is returned and the event is not updated.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ical::utils::*;
    /// use ical::{VCalendar, VEvent, Uid, ical};
    ///
    /// let mut cal = VCalendar::new("test");
    ///
    /// cal.add_event(VEvent::new("1", dt("20180101T120000Z")))
    ///    .unwrap();
    ///
    /// cal.edit_event(0, |event| {
    ///     event.uid = Some("2".into());
    /// })
    /// .unwrap();
    ///
    /// assert_eq!("2", cal.events()[0].uid.as_ref().unwrap().value.as_str());
    /// ```
    pub fn edit_event(&mut self, idx: usize, f: impl FnOnce(&mut VEvent)) -> Result<()> {
        let mut event = self
            .events
            .get_mut(idx)
            .ok_or(Error::MissingEvent(idx))?
            .clone();

        f(&mut event);

        event
            .validate(self, Some(idx))
            .into_result(|viol| Violation::InvalidEvent(idx, viol))?;

        self.events[idx] = event;

        Ok(())
    }

    /// Removes event from the calendar.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ical::utils::*;
    /// use ical::{VCalendar, VEvent, Uid, ical};
    ///
    /// let mut cal = VCalendar::new("test");
    ///
    /// cal.add_event(VEvent::new("1", dt("20180101T120000Z")))
    ///    .unwrap();
    ///
    /// let event = cal.remove_event(0).unwrap();
    ///
    /// assert_eq!("1", event.uid.as_ref().unwrap().value.as_str());
    /// assert_eq!(0, cal.events().len());
    /// ```
    pub fn remove_event(&mut self, idx: usize) -> Result<VEvent> {
        if idx < self.events.len() {
            Ok(self.events.remove(idx))
        } else {
            Err(Error::MissingEvent(idx))
        }
    }

    #[must_use]
    pub fn timezones(&self) -> &[VTimeZone] {
        &self.timezones
    }

    // TODO docs
    pub fn add_timezone(&mut self, timezone: VTimeZone) -> Result<()> {
        timezone
            .validate(self, None)
            .into_result(|viol| Violation::InvalidTimeZone(self.timezones.len(), viol))?;

        self.timezones.push(timezone);

        Ok(())
    }

    // TODO docs
    pub fn with_timezone(mut self, timezone: VTimeZone) -> Result<Self> {
        self.add_timezone(timezone)?;

        Ok(self)
    }

    // TODO docs
    pub fn edit_timezone(&mut self, idx: usize, f: impl FnOnce(&mut VTimeZone)) -> Result<()> {
        let mut timezone = self
            .timezones
            .get_mut(idx)
            .ok_or(Error::MissingTimeZone(idx))?
            .clone();

        f(&mut timezone);

        timezone
            .validate(self, Some(idx))
            .into_result(|viol| Violation::InvalidTimeZone(idx, viol))?;

        self.timezones[idx] = timezone;

        Ok(())
    }

    /// Converts given string into a calendar.
    ///
    /// See [`Self::from_bytes()`] for more details.
    #[allow(
        clippy::should_implement_trait,
        reason = "can't actually do `impl FromStr for (Self, Vec<...>)`"
    )]
    pub fn from_str(src: &str) -> Result<(Self, Vec<ReadMsg>)> {
        Self::from_bytes(src.as_bytes())
    }

    /// Converts given byte-slice into a calendar.
    ///
    /// This function accepts a byte-slice instead of a string, because it's
    /// possible for an iCal-string to contain improperly-split Unicode
    /// characters and this function tries to recover those.
    ///
    /// This function returns both the parsed calendar and a list of messages
    /// describing errors and/or warnings stumbled upon during parsing - we try
    /// to recover as much information as possible from the string, even if it's
    /// malformed.
    ///
    /// See also [`Self::from_str()`].
    pub fn from_bytes(src: &[u8]) -> Result<(Self, Vec<ReadMsg>)> {
        let mut r = Reader::new(src);
        let mut this = None;

        while !r.is_empty() {
            let Some(e) = r.entry() else {
                // TODO error: trailing data
                break;
            };

            if !e.try_comp(&mut r, "VCALENDAR", &mut this) {
                e.burn(&mut r);
            }
        }

        let this = this.ok_or_else(|| Error::viol([Violation::MissingCalendar]))?;
        let msgs = r.finish();

        Ok((this, msgs))
    }

    /// Converts this calendar into an iCal string.
    ///
    /// See also: [`Self::from_str()`].
    #[must_use]
    #[allow(
        clippy::inherent_to_string,
        reason = "we don't have `impl FromStr`, so for documentation purposes let's keep .to_string() associated as well"
    )]
    pub fn to_string(&self) -> String {
        let mut w = Writer::default();

        w.comp("VCALENDAR", self);
        w.finish()
    }
}

impl Read<Component> for VCalendar {
    fn read(r: &mut Reader) -> Option<Self> {
        let mut method = None;
        let mut prodid = None;
        let mut version = None;
        let mut calscale = None;
        let mut events: Vec<Spanned<VEvent>> = Vec::new();
        let mut timezones: Vec<Spanned<VTimeZone>> = Vec::new();

        while let Some(e) = r.entry() {
            if e.try_prop(r, "METHOD", &mut method)
                || e.try_prop(r, "PRODID", &mut prodid)
                || e.try_prop(r, "VERSION", &mut version)
                || e.try_prop(r, "CALSCALE", &mut calscale)
                || e.try_comps(r, "VEVENT", &mut events)
                || e.try_comps(r, "VTIMEZONE", &mut timezones)
            {
                continue;
            }

            if e.try_comp_end("VCALENDAR") {
                break;
            }

            e.burn(r);
        }

        let prodid = r.unwrap_prop("PRODID", prodid);
        let version = r.unwrap_prop("VERSION", version);

        let mut this = Self {
            method,
            prodid: prodid?,
            version: version?,
            calscale: calscale.unwrap_or_default(),
            events: Vec::new(),
            timezones: Vec::new(),
        };

        for (idx, Spanned { span, value }) in timezones.into_iter().enumerate() {
            for viol in value.validate(&this, None) {
                r.viol(span, Violation::InvalidTimeZone(idx, viol));
            }

            this.timezones.push(value);
        }

        for (idx, Spanned { span, value }) in events.into_iter().enumerate() {
            for viol in value.validate(&this, None) {
                r.viol(span, Violation::InvalidEvent(idx, viol));
            }

            this.events.push(value);
        }

        Some(this)
    }
}

impl Write<Component> for VCalendar {
    fn write(&self, w: &mut Writer) {
        w.prop_opt("METHOD", self.method.as_ref());
        w.prop("PRODID", &self.prodid);
        w.prop("VERSION", self.version);
        w.prop("CALSCALE", self.calscale);

        for timezone in &self.timezones {
            w.comp("VTIMEZONE", timezone);
        }

        for event in &self.events {
            w.comp("VEVENT", event);
        }
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;
    use ext_php_rs::{binary_slice::BinarySlice, prelude::*};

    #[derive(Clone, Debug, ZvalConvert)]
    struct PhpParseResult {
        calendar: VCalendar,
        messages: Vec<PhpParseMessage>,
    }

    #[derive(Clone, Debug, ZvalConvert)]
    struct PhpParseMessage {
        kind: String,
        text: String,
    }

    /// Creates a new, minimal [`VCalendar`].
    #[php_function]
    fn ical_new(prodid: String) -> VCalendar {
        VCalendar::new(prodid)
    }

    /// Creates a new, minimal [`VEvent`].
    #[php_function]
    fn ical_new_event(uid: String, dtstamp: DateTime) -> VEvent {
        VEvent::new(uid, dtstamp)
    }

    /// Converts given string into a calendar.
    ///
    /// This function throws an exception if the output could't have been parsed
    /// whatsoever - otherwise, if anything got parsed correctly, an object with
    /// the calendar and possible parse errors/warnings is returned.
    #[allow(clippy::needless_pass_by_value)]
    #[php_function]
    fn ical_parse(src: BinarySlice<u8>) -> Result<PhpParseResult, String> {
        let (calendar, messages) = VCalendar::from_bytes(&src).map_err(|err| err.to_string())?;

        let messages = messages
            .into_iter()
            .map(|msg| PhpParseMessage {
                kind: match msg.kind {
                    ReadMsgKind::Warning => "Warning".into(),
                    ReadMsgKind::Error => "Error".into(),
                    ReadMsgKind::Violation => "Violation".into(),
                },
                text: msg.to_string(&**src),
            })
            .collect();

        Ok(PhpParseResult { calendar, messages })
    }

    /// Converts given calendar into a string.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    #[php_function]
    fn ical_print(src: VCalendar) -> String {
        src.to_string()
    }

    /// Creates an iCal-compatible string (with correct newlines).
    ///
    /// Sanitizing input is not necessary before calling [`ical_parse()`], since
    /// the parser will handle bare `\n` newlines just fine.
    ///
    /// Rather, this function is useful when you'd like to *compare* a string
    /// returned from [`ical_print()`] with a string hardcoded into PHP, as the
    /// latter will contain just `\n`-newlines, while the string returned from
    /// [`ical_print()`] will be delimited using `\r\n`.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    #[php_function]
    fn ical_sanitize(src: String) -> String {
        let mut src = src.lines().collect::<Vec<_>>().join("\r\n");

        src.push_str("\r\n");
        src
    }

    #[php_module]
    fn get_module(module: ModuleBuilder) -> ModuleBuilder {
        module
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;

    fn event(id: &str) -> VEvent {
        VEvent::new(id, dt("20180101T120000Z"))
    }

    fn tz(id: &str) -> VTimeZone {
        VTimeZone::new_standard(
            id,
            TzProps::new(
                dt("19700329T020000"),
                UtcOffset::new(Sign::Pos, 1, 0, 0).unwrap(),
                UtcOffset::new(Sign::Pos, 2, 0, 0).unwrap(),
            ),
        )
    }

    #[test]
    fn with_method() {
        assert_eq!(cal().method(), None);

        assert_eq!(
            cal().with_method(Method::Publish).method(),
            Some(Method::Publish)
        );
    }

    #[test]
    fn with_event() {
        let target = cal().with_event(event("1")).unwrap();

        assert_eq!(1, target.events().len());
    }

    #[test]
    fn with_duplicated_event() {
        let actual = cal()
            .with_event(event("1"))
            .unwrap()
            .with_event(event("1"))
            .unwrap_err();

        let expected = Error::viol([Violation::InvalidEvent(
            1,
            VEventViolation::DuplicatedUid("1".into()),
        )]);

        assert_eq!(expected, actual);
    }

    #[test]
    fn edit_event() {
        let mut target = cal().with_event(event("1")).unwrap();

        assert_eq!(1, target.events.len());

        assert_eq!(
            1,
            target.events[0]
                .dtstamp
                .as_ref()
                .unwrap()
                .value
                .date
                .day()
                .as_num()
        );

        target
            .edit_event(0, |event| {
                event.dtstamp = Some(dt("20180102T120000Z").into());
            })
            .unwrap();

        assert_eq!(1, target.events.len());

        assert_eq!(
            2,
            target.events[0]
                .dtstamp
                .as_ref()
                .unwrap()
                .value
                .date
                .day()
                .as_num()
        );
    }

    #[test]
    fn edit_missing_event() {
        let actual = cal().edit_event(123, |_| ()).unwrap_err();
        let expected = Error::MissingEvent(123);

        assert_eq!(expected, actual);
    }

    #[test]
    fn edit_event_and_make_it_duplicated() {
        let mut target = cal()
            .with_event(event("1"))
            .unwrap()
            .with_event(event("2"))
            .unwrap();

        let actual = target
            .edit_event(0, |event| {
                event.uid = Some("2".into());
            })
            .unwrap_err();

        let expected = Error::viol([Violation::InvalidEvent(
            0,
            VEventViolation::DuplicatedUid("2".into()),
        )]);

        assert_eq!(expected, actual);
    }

    #[test]
    fn with_timezone() {
        let target = cal().with_timezone(tz("Europe/Vatican")).unwrap();

        assert_eq!(1, target.timezones.len());
    }

    #[test]
    fn with_duplicated_timezone() {
        let actual = cal()
            .with_timezone(tz("Europe/Vatican"))
            .unwrap()
            .with_timezone(tz("Europe/Vatican"))
            .unwrap_err();

        let expected = Error::viol([Violation::InvalidTimeZone(
            1,
            VTimeZoneViolation::DuplicatedId("Europe/Vatican".into()),
        )]);

        assert_eq!(expected, actual);
    }

    #[test]
    fn edit_timezone() {
        let mut target = cal().with_timezone(tz("Europe/Vatican")).unwrap();

        assert_eq!(1, target.timezones.len());
        assert_eq!(0, target.timezones[0].daylights.len());

        target
            .edit_timezone(0, |tz| {
                tz.daylights = tz.standards.clone();
            })
            .unwrap();

        assert_eq!(1, target.timezones.len());
        assert_eq!(1, target.timezones[0].daylights.len());
    }

    #[test]
    fn edit_missing_timezone() {
        let actual = cal().edit_timezone(123, |_| ()).unwrap_err();
        let expected = Error::MissingTimeZone(123);

        assert_eq!(expected, actual);
    }

    #[test]
    fn edit_timezone_and_make_it_duplicated() {
        let mut target = cal()
            .with_timezone(tz("Europe/Warsaw"))
            .unwrap()
            .with_timezone(tz("Europe/Stockholm"))
            .unwrap();

        let actual = target
            .edit_timezone(0, |tz| {
                tz.tzid = "Europe/Stockholm".into();
            })
            .unwrap_err();

        let expected = Error::viol([Violation::InvalidTimeZone(
            0,
            VTimeZoneViolation::DuplicatedId("Europe/Stockholm".into()),
        )]);

        assert_eq!(expected, actual);
    }
}
