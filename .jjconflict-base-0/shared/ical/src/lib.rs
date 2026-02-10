#![allow(
    clippy::wildcard_imports,
    reason = "it's just waay more convenient this way - especially around objects such as VEvent which import half of the crate"
)]

mod io;
mod objects;
mod result;
pub mod utils;

pub use self::io::*;
pub use self::objects::*;
pub use self::result::*;

/// Calendar, as described in RFC5545.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.4>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ext_php_rs::ZvalConvert))]
pub struct VCalendar {
    pub method: Option<Method>,
    pub prodid: ProdId,
    pub version: Version,
    pub calscale: CalScale,
    pub events: Vec<VEvent>,
    pub timezones: Vec<VTimeZone>,
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
    pub fn with_method(mut self, method: Method) -> Self {
        self.method = Some(method);
        self
    }

    #[must_use]
    pub fn with_event(mut self, event: VEvent) -> Self {
        self.events.push(event);
        self
    }

    #[must_use]
    pub fn with_events(mut self, events: impl IntoIterator<Item = VEvent>) -> Self {
        self.events.extend(events);
        self
    }

    #[must_use]
    pub fn with_timezone(mut self, timezone: VTimeZone) -> Self {
        self.timezones.push(timezone);
        self
    }

    #[must_use]
    pub fn with_timezones(mut self, timezones: impl IntoIterator<Item = VTimeZone>) -> Self {
        self.timezones.extend(timezones);
        self
    }

    /// Converts given string into a calendar.
    ///
    /// See [`Self::from_bytes()`] for more details.
    #[allow(clippy::should_implement_trait, reason = "important doc-comment")]
    pub fn from_str(src: &str) -> Result<ParsedVCalendar> {
        Self::from_bytes(src.as_bytes())
    }

    /// Converts given byte-slice into a calendar.
    ///
    /// This function accepts a byte-slice, because it's possible for the input
    /// data to contain improperly split Unicode characters, which this function
    /// tries to recover.
    ///
    /// (going through `&str` would force the input to be a Unicode string,
    /// yielding such recovery impossible.)
    ///
    /// Note that this function doesn't return the calendar directly - it
    /// returns a struct containing the calendar, parser messages, and validator
    /// messages that describe problems with the input data, if any.
    ///
    /// See also [`Self::from_str()`].
    pub fn from_bytes(src: &[u8]) -> Result<ParsedVCalendar> {
        let mut r = IcsReader::new(src);
        let mut cal: Option<Self> = None;

        while !r.is_empty() {
            let Some(e) = r.entry() else {
                break;
            };

            if !e.try_comp(&mut r, "VCALENDAR", &mut cal) {
                _ = e.burn(&mut r, Kind::Component);
            }
        }

        let mut msgs = r.finish();

        let Some(cal) = cal else {
            if msgs.iter().any(|msg| msg.kind.is_error()) {
                // If we've already gotten some errors, let's avoid piling up an
                // extra "missing calendar" - that's because most likely the
                // reason why we're missing the calendar *is* one of those other
                // errors (e.g. there's an invalid syntax somewhere)
            } else {
                msgs.push(ReadMsg {
                    at: None,
                    body: "missing calendar".into(),
                    kind: ReadMsgKind::Error,
                    context: Vec::new(),
                });
            }

            return Err(Error::InvalidIcs(msgs));
        };

        let viols = cal.validate().into_viols();

        Ok(ParsedVCalendar { cal, msgs, viols })
    }

    /// Validates that the calendar adheres to RFC (e.g. if a date mentiones a
    /// time zone, we check that calendar actually contains this time zone).
    ///
    /// Calling this function is required to convert calendar into a string,
    /// though note that a calendar which fails the validation can still be
    /// converted into a string, just with less guarantees regarding the
    /// comaptibility.
    ///
    /// See [`CleanVCalendar::to_string()`] and [`DirtyVCalendar::to_string()`].
    #[doc(alias = "to_string")]
    #[must_use]
    pub fn validate(&self) -> ValidatedVCalendar<'_> {
        let mut viols = Vec::new();

        for (idx, event) in self.events.iter().enumerate() {
            viols.extend(
                event
                    .validate(self, Some(idx))
                    .into_iter()
                    .map(|viol| Violation::InvalidEvent(idx, viol)),
            );
        }

        for (idx, tz) in self.timezones.iter().enumerate() {
            viols.extend(
                tz.validate(self, Some(idx))
                    .into_iter()
                    .map(|viol| Violation::InvalidTimeZone(idx, viol)),
            );
        }

        if viols.is_empty() {
            ValidatedVCalendar::Clean(CleanVCalendar { cal: self })
        } else {
            ValidatedVCalendar::Dirty(DirtyVCalendar { cal: self, viols })
        }
    }

    #[must_use]
    #[allow(
        clippy::inherent_to_string,
        reason = "we want users to go through .validate()"
    )]
    fn to_string(&self) -> String {
        let mut w = IcsWriter::default();

        w.comp("VCALENDAR", self);
        w.finish()
    }
}

impl IcsRead<Component> for VCalendar {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut method = None;
        let mut prodid = None;
        let mut version = None;
        let mut calscale = None;
        let mut events = Vec::new();
        let mut timezones = Vec::new();

        loop {
            let e = r.entry()?;

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

            e.burn(r, Kind::Component)?;
        }

        let prodid = r.unwrap_prop("PRODID", prodid);
        let version = r.unwrap_prop("VERSION", version);

        Some(Self {
            method,
            prodid: prodid?,
            version: version?,
            calscale: calscale.unwrap_or_default(),
            events,
            timezones,
        })
    }
}

impl IcsWrite<Component> for VCalendar {
    fn write(&self, w: &mut IcsWriter) {
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

/// Outcome of calendar parsing, see [`VCalendar::from_str()`] or
/// [`VCalendar::from_bytes()`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedVCalendar {
    pub cal: VCalendar,

    /// Parsing messages (e.g. a syntax error somewhere in the file).
    pub msgs: Vec<ReadMsg>,

    /// Validation messages (e.g. misconfigured event).
    pub viols: Vec<Violation>,
}

/// Outcome of calendar validation, see [`VCalendar::validate()`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidatedVCalendar<'a> {
    Clean(CleanVCalendar<'a>),
    Dirty(DirtyVCalendar<'a>),
}

impl<'a> ValidatedVCalendar<'a> {
    /// Converts this enum into [`CleanVCalendar`].
    #[must_use]
    pub fn into_clean(self) -> Option<CleanVCalendar<'a>> {
        match self {
            ValidatedVCalendar::Clean(this) => Some(this),
            ValidatedVCalendar::Dirty(_) => None,
        }
    }

    /// Converts this enum into [`DirtyVCalendar`].
    #[must_use]
    pub fn into_dirty(self) -> Option<DirtyVCalendar<'a>> {
        match self {
            ValidatedVCalendar::Clean(_) => None,
            ValidatedVCalendar::Dirty(this) => Some(this),
        }
    }

    /// Returns the underlying calendar.
    #[must_use]
    pub fn cal(&self) -> &'a VCalendar {
        match self {
            ValidatedVCalendar::Clean(this) => this.cal(),
            ValidatedVCalendar::Dirty(this) => this.cal(),
        }
    }

    /// Returns the validation errors, if any.
    #[must_use]
    pub fn viols(&self) -> &[Violation] {
        match self {
            ValidatedVCalendar::Clean(_) => &[],
            ValidatedVCalendar::Dirty(this) => this.viols(),
        }
    }

    /// Returns the validation errors, if any.
    #[must_use]
    pub fn into_viols(self) -> Vec<Violation> {
        match self {
            ValidatedVCalendar::Clean(_) => Vec::new(),
            ValidatedVCalendar::Dirty(this) => this.into_viols(),
        }
    }
}

/// [`VCalendar`] that passed the validation, see [`ValidatedVCalendar`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanVCalendar<'a> {
    cal: &'a VCalendar,
}

impl<'a> CleanVCalendar<'a> {
    /// Returns the underlying calendar.
    #[must_use]
    pub fn cal(&self) -> &'a VCalendar {
        self.cal
    }

    /// Converts this calendar into a string.
    ///
    /// Since this calendar has passed the validation, it's guaranteed (modulo
    /// bugs within this crate) that the returned string is RFC-compliant and
    /// should be properly understood by all other iCal implementations.
    #[must_use]
    #[allow(clippy::inherent_to_string, reason = "important doc-comment")]
    pub fn to_string(&self) -> String {
        self.cal.to_string()
    }
}

/// [`VCalendar`] that failed to pass the validation, see
/// [`ValidatedVCalendar`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtyVCalendar<'a> {
    cal: &'a VCalendar,
    viols: Vec<Violation>,
}

impl<'a> DirtyVCalendar<'a> {
    /// Returns the underlying calendar.
    #[must_use]
    pub fn cal(&self) -> &'a VCalendar {
        self.cal
    }

    /// Returns the validation errors.
    ///
    /// Returned slice is guaranteed to contain at least one item.
    #[must_use]
    pub fn viols(&self) -> &[Violation] {
        &self.viols
    }

    /// Returns the validation errors.
    ///
    /// Returned vector is guaranteed to contain at least one item.
    #[must_use]
    pub fn into_viols(self) -> Vec<Violation> {
        self.viols
    }

    /// Converts this calendar into a string, at your own risk.
    ///
    /// Since this calendar has failed to pass the validation, it is *not*
    /// guaranteed that the returned string will be properly understood by
    /// another iCal implementation.
    ///
    /// For instance, the calendar might be missing a `VTIMEZONE` object and
    /// implementations are free to reject such calendars.
    ///
    /// Still, we allow for this conversion, because it might be handy for
    /// debugging purposes and so on. This crate itself is also able to parse
    /// "dirty calendars", so you should be able to pass the returned string to
    /// [`VCalendar::from_str()`] and get a valid (though still dirty) calendar
    /// back.
    #[must_use]
    #[allow(clippy::inherent_to_string, reason = "important doc-comment")]
    pub fn to_string(&self) -> String {
        self.cal.to_string()
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
        let ParsedVCalendar { cal, msgs, viols } =
            VCalendar::from_bytes(&src).map_err(|err| err.to_string())?;

        let msgs = msgs.into_iter().map(|msg| PhpParseMessage {
            kind: match msg.kind {
                ReadMsgKind::Warning => "Warning".into(),
                ReadMsgKind::Error => "Error".into(),
            },
            text: msg.to_string(),
        });

        let viols = viols.into_iter().map(|msg| PhpParseMessage {
            kind: "Violation".into(),
            text: msg.to_string(),
        });

        Ok(PhpParseResult {
            calendar: cal,
            messages: msgs.chain(viols).collect(),
        })
    }

    /// Converts given calendar into a string.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    #[php_function]
    fn ical_print(src: VCalendar) -> String {
        src.to_string()
    }

    /// Creates an *.ics string.
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
            .function(wrap_function!(ical_new))
            .function(wrap_function!(ical_new_event))
            .function(wrap_function!(ical_parse))
            .function(wrap_function!(ical_print))
            .function(wrap_function!(ical_sanitize))
    }
}

#[cfg(test)]
mod tests {
    // Covered via ../tests/acceptance.rs
}
