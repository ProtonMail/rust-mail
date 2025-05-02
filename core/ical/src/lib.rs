#![allow(
    clippy::wildcard_imports,
    reason = "it's just waay more convenient this way - especially around objects such as VEvent which import half of the crate"
)]

mod objects;

pub use self::objects::*;

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
}
