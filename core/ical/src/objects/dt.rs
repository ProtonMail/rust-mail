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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date() {
        assert_trip!("20180101", Date as Value);
        assert_trip!("19980101", Date as Value);
        assert_trip!("12341210", Date as Value);
    }

    #[test]
    fn time() {
        assert_trip!("000000", Time as Value);
        assert_trip!("000030", Time as Value);
        assert_trip!("000059", Time as Value);
        assert_trip!("000060", Time as Value);
        assert_trip!("010203", Time as Value);
        assert_trip!("120000", Time as Value);
        assert_trip!("235959", Time as Value);
    }

    #[test]
    fn date_time() {
        assert_trip!(":20180101T123456", DateTime as Property);
        assert_trip!(":20180101T123456Z", DateTime as Property);
        assert_trip!(";TZID=Europe/Warsaw:20180101T123456", DateTime as Property);

        // ---

        assert_trip!("20180101T123456", DateTime<UtcOrLocalForm> as Value);
        assert_trip!("20180101T123456Z", DateTime<UtcOrLocalForm> as Value);

        // ---

        assert_trip!("20180101T123456Z", DateTime<UtcForm> as Value);

        assert_trip!(
            "20180101T123456" => "20180101T123456Z", yielding [
                ReadMsg {
                    at: Some(Span::new(15, 16)),
                    msg: "expected utc-date-time (missing `Z` here)".into(),
                    kind: ReadMsgKind::Warning,
                    context: Vec::new(),
                },
            ],
            DateTime<UtcForm> as Value
        );
    }

    #[test]
    fn date_or_date_time() {
        assert_trip!(";VALUE=DATE:20180101", DateOrDt as Property);
        assert_trip!(":20180101T120000Z", DateOrDt as Property);
        assert_trip!(";TZID=Europe/Warsaw:20180101T120000", DateOrDt as Property);

        assert_trip!(
            ";VALUE=DATE:20180101T000000" => ";VALUE=DATE:20180101", yielding [
                ReadMsg {
                    at: Some(Span::new(21, 27)),
                    msg: "non-conformant: skipping T000000 to coerce this date-time into date".into(),
                    kind: ReadMsgKind::Warning,
                    context: Vec::new(),
                },
            ],
            DateOrDt as Property
        );
    }

    #[test]
    fn viol_unknown_day() {
        let actual = Date::new(
            Year::new(2018).unwrap(),
            Month::new(2).unwrap(),
            Day::new(30).unwrap(),
        );

        let expected = Err(DateTimeViolation::UnknownDay {
            year: 2018,
            month: 2,
            day: 30,
        });

        assert_eq!(expected, actual);
    }
}
