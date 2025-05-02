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
    pub fn with_timezone(mut self, tz: VTimeZone) -> Self {
        self.timezones.push(tz);
        self
    }

    /// Converts given string into a calendar.
    ///
    /// See [`Self::from_bytes()`] for more details.
    #[allow(
        clippy::should_implement_trait,
        reason = "can't actually do `impl FromStr for (Self, Vec<...>)`"
    )]
    #[must_use]
    pub fn from_str(src: &str) -> Option<(Self, Vec<ReadMsg>)> {
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
    #[must_use]
    pub fn from_bytes(src: &[u8]) -> Option<(Self, Vec<ReadMsg>)> {
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

        let this = this?;
        let msgs = r.finish();

        Some((this, msgs))
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
        let mut events = Vec::new();
        let mut timezones = Vec::new();

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
