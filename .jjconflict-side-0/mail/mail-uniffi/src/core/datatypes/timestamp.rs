use chrono::{DateTime, TimeZone};
use proton_core_common::datatypes::UnixTimestamp as RealTimestamp;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnixTimestamp(pub u64);

uniffi::custom_newtype!(UnixTimestamp, u64);

impl From<RealTimestamp> for UnixTimestamp {
    fn from(t: RealTimestamp) -> Self {
        Self(t.as_u64())
    }
}

impl From<UnixTimestamp> for RealTimestamp {
    fn from(t: UnixTimestamp) -> Self {
        Self::new(t.0)
    }
}
impl<Tz: TimeZone> From<DateTime<Tz>> for UnixTimestamp {
    fn from(value: DateTime<Tz>) -> Self {
        RealTimestamp::from(value).into()
    }
}
