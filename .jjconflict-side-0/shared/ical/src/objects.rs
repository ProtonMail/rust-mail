mod attendee;
mod caladdress;
mod calscale;
mod class;
mod cn;
mod created;
mod cutype;
mod description;
mod dt;
mod dtend;
mod dtstamp;
mod dtstart;
mod duration;
mod email;
mod exdate;
mod location;
mod method;
mod organizer;
mod paramvalue;
mod partstat;
mod priority;
mod prodid;
mod recur;
mod recurrenceid;
mod repeat;
mod role;
mod rrule;
mod rsvp;
mod sentby;
mod sequence;
mod signed;
mod status;
mod summary;
mod text;
mod transp;
mod trigger;
mod tzid;
mod tzname;
mod tzoffsetfrom;
mod tzoffsetto;
mod uid;
mod utcoffset;
mod valarm;
mod version;
mod vevent;
mod vtimezone;

use super::*;
use itertools::Itertools;
use jiff::civil::{
    Date as JiffDate, DateTime as JiffDateTime, Time as JiffTime, Weekday as JiffWeekday,
};
use jiff::tz::TimeZone as JiffTimeZone;
use jiff::{Error as JiffError, Span as JiffSpan, Unit as JiffUnit, Zoned as JiffZoned};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::num::NonZeroI8;
use strum::EnumString;
use thiserror::Error;

#[cfg(feature = "php")]
use ext_php_rs::{
    convert::FromZval as FromPhpZval, convert::IntoZval as IntoPhpZval, error::Result as PhpResult,
    flags::DataType as PhpDataType, prelude::*, types::Zval as PhpZval,
};

pub use self::attendee::*;
pub use self::caladdress::*;
pub use self::calscale::*;
pub use self::class::*;
pub use self::cn::*;
pub use self::created::*;
pub use self::cutype::*;
pub use self::description::*;
pub use self::dt::*;
pub use self::dtend::*;
pub use self::dtstamp::*;
pub use self::dtstart::*;
pub use self::duration::*;
pub use self::email::*;
pub use self::exdate::*;
pub use self::location::*;
pub use self::method::*;
pub use self::organizer::*;
pub use self::paramvalue::*;
pub use self::partstat::*;
pub use self::priority::*;
pub use self::prodid::*;
pub use self::recur::*;
pub use self::recurrenceid::*;
pub use self::repeat::*;
pub use self::role::*;
pub use self::rrule::*;
pub use self::rsvp::*;
pub use self::sentby::*;
pub use self::sequence::*;
pub use self::signed::*;
pub use self::status::*;
pub use self::summary::*;
pub use self::text::*;
pub use self::transp::*;
pub use self::trigger::*;
pub use self::tzid::*;
pub use self::tzname::*;
pub use self::tzoffsetfrom::*;
pub use self::tzoffsetto::*;
pub use self::uid::*;
pub use self::utcoffset::*;
pub use self::valarm::*;
pub use self::version::*;
pub use self::vevent::*;
pub use self::vtimezone::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Component;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Property;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Component,
    Property,
}
