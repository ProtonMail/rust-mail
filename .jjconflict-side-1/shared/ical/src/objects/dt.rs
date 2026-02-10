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

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DateTimeViolation {
    #[error("time zone `{0}` is not known")]
    UnknownTimeZone(String),

    #[error("day {year:04}-{month:02}-{day:02} (yyyy-mm-dd) does not exist")]
    UnknownDay { year: u32, month: u32, day: u32 },
}

#[derive(Debug, Error)]
pub enum DateTimeError {
    #[error("can't convert {0} into {1}")]
    InvalidConversion(&'static str, &'static str),

    #[error("can't infer time zone from `{0}`")]
    UnknownTimeZone(JiffZoned),

    #[error("{0}")]
    Jiff(#[from] JiffError),
}
