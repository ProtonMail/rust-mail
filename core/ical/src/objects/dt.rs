mod date;
mod date_or_dt;
mod date_time;
mod forms;
mod time;
mod units;

pub use self::date::*;
pub use self::date_or_dt::*;
pub use self::date_time::*;
pub use self::forms::*;
pub use self::time::*;
pub use self::units::*;
use super::*;

#[allow(unused)]
pub(crate) trait FromJiffZoned
where
    Self: Sized,
{
    fn from_jiff(jiff: JiffZoned) -> Option<Self>;
}

pub(crate) trait AsJiffZoned {
    fn as_jiff(&self) -> Result<JiffZoned, JiffError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DateTimeViolation {
    #[error("time zone `{0}` is not known")]
    UnknownTimeZone(String),

    #[error("day {year:04}-{month:02}-{day:02} (yyyy-mm-dd) does not exist")]
    UnknownDay { year: u32, month: u32, day: u32 },
}
