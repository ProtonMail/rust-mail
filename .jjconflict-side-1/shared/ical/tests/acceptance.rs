use insta::assert_snapshot;
use jiff::Zoned;
use pretty_assertions as pa;
use proton_ical::*;
use std::fmt::Write;
use std::{fs, path::Path};
use test_case::test_case;

#[test]
fn empty() {
    let cal = VCalendar::new("-//Proton AG//iCal//EN");

    let str = ics! {"
        BEGIN:VCALENDAR
        PRODID:-//Proton AG//iCal//EN
        VERSION:2.0
        CALSCALE:GREGORIAN
        END:VCALENDAR
    "};

    assert(&cal, &str);
}

#[test]
fn with_event() {
    let cal = VCalendar::new("-//Proton AG//iCal//EN").with_event(
        VEvent::new(
            "0000-0000-0000-0001",
            DateTime {
                date: Date::new_unchecked(2024, 1, 1),
                time: Time::new_unchecked(12, 0, 0),
                form: AnyForm::Local,
            },
        )
        .with_dtstart(DateTime {
            date: Date::new_unchecked(2024, 1, 1),
            time: Time::new_unchecked(10, 0, 0),
            form: AnyForm::Local,
        })
        .with_rrule(Recur::new(Freq::Daily).with_count(5))
        .with_alarm(EmailAlarm::new(
            Trigger::start(Duration::neg(TimeDuration::minutes(10))),
            "reminder before the meeting!",
            "just a reminder",
            EmailAddress::from("someone@localhost"),
        )),
    );

    let str = ics! {"
        BEGIN:VCALENDAR
        PRODID:-//Proton AG//iCal//EN
        VERSION:2.0
        CALSCALE:GREGORIAN
        BEGIN:VEVENT
        UID:0000-0000-0000-0001
        DTSTAMP:20240101T120000
        DTSTART:20240101T100000
        RRULE:FREQ=DAILY;COUNT=5
        BEGIN:VALARM
        ACTION:EMAIL
        TRIGGER:-PT10M
        DESCRIPTION:reminder before the meeting!
        SUMMARY:just a reminder
        ATTENDEE:mailto:someone@localhost
        END:VALARM
        END:VEVENT
        END:VCALENDAR
    "};

    assert(&cal, &str);
}

#[test]
fn with_method() {
    let cal = VCalendar::new("-//Proton AG//iCal//EN").with_method(Method::Publish);

    let str = ics! {"
        BEGIN:VCALENDAR
        METHOD:PUBLISH
        PRODID:-//Proton AG//iCal//EN
        VERSION:2.0
        CALSCALE:GREGORIAN
        END:VCALENDAR
    "};

    assert(&cal, &str);
}

#[test]
fn without_calscale() {
    let out = VCalendar::from_str(&ics! {"
        BEGIN:VCALENDAR
        PRODID:-//Proton AG//iCal//EN
        VERSION:2.0
        END:VCALENDAR
    "})
    .unwrap();

    assert_eq!(CalScale::Gregorian, out.cal.calscale);
    assert!(out.msgs.is_empty());
    assert!(out.viols.is_empty());
}

#[test]
fn vtimezone() {
    let out = VTimeZone::from_str(&ics! {"
        BEGIN:VTIMEZONE
        TZID:Breaking/Bad
        BEGIN:STANDARD
        TZNAME:GMT
        TZOFFSETFROM:+0100
        TZOFFSETTO:+0000
        DTSTART:19701025T020000
        END:STANDARD
        END:VTIMEZONE
    "})
    .unwrap();

    assert!(out.msgs.is_empty());
    assert_eq!("Breaking/Bad", out.tz.tzid.value.as_str());
    assert_eq!(1, out.tz.standards.len());

    // ---

    let actual = VTimeZone::from_str(&ics! {"
        BEGIN:VTIMEZONE
        END:VTIMEZONE
    "})
    .unwrap_err()
    .as_invalid_ics()
    .unwrap()
    .to_owned();

    assert_eq!(1, actual.len());
    assert_eq!("missing property `TZID`", actual[0].body);

    // ---

    let actual = VTimeZone::from_str("")
        .unwrap_err()
        .as_invalid_ics()
        .unwrap()
        .to_owned();

    assert_eq!(1, actual.len());
    assert_eq!("missing time zone", actual[0].body);
}

#[test]
fn with_microsoft_timezone() {
    let out = VCalendar::from_str(&ics! {"
        BEGIN:VCALENDAR
        METHOD:REQUEST
        PRODID:Microsoft Exchange Server 2010
        VERSION:2.0
        BEGIN:VTIMEZONE
        TZID:Central European Standard Time
        BEGIN:STANDARD
        DTSTART:16010101T030000
        TZOFFSETFROM:+0200
        TZOFFSETTO:+0100
        RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=10
        END:STANDARD
        BEGIN:DAYLIGHT
        DTSTART:16010101T020000
        TZOFFSETFROM:+0100
        TZOFFSETTO:+0200
        RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=3
        END:DAYLIGHT
        END:VTIMEZONE
        BEGIN:VEVENT
        UID:1234
        SUMMARY:outlook
        DTSTAMP;TZID=Central European Standard Time:20180101T123000
        DTSTART;TZID=Central European Standard Time:20180101T123000
        DTEND;TZID=Central European Standard Time:20180101T130000
        END:VEVENT
        END:VCALENDAR
    "})
    .unwrap();

    assert!(out.msgs.is_empty());
    assert!(out.viols.is_empty());

    let dtstart: Zoned = out.cal.events[0]
        .dtstart
        .clone()
        .unwrap()
        .value
        .try_into()
        .unwrap();

    assert_eq!(
        "2018-01-01T12:30:00+01:00[Europe/Warsaw]",
        dtstart.to_string()
    );
}

/// Make sure we can parse various atypical and funny cases.
///
/// Fixtures here were taken (mostly) from the surgery dataset, but since we
/// don't have the surgery logic here yet, what we do is that we simply parse
/// the files and make sure the output looks reasonable enough.
#[test_case("broken-attendee-1")]
#[test_case("broken-attendee-1-email")]
#[test_case("broken-organizer-broken-cn-non-strict")]
#[test_case("broken-organizer-broken-cn-strict")]
#[test_case("broken-param")]
#[test_case("dtend-before-dtstart")]
#[test_case("duration-dst")]
#[test_case("floating-no-wr-timezone")]
#[test_case("floating-time")]
#[test_case("floating-time-pm")]
#[test_case("floating-time-zulu")]
#[test_case("format-exdate")]
#[test_case("format-for-full-day-event")]
#[test_case("long-event-description")]
#[test_case("long-event-description-special-chars")]
#[test_case("lower-case-tz")]
#[test_case("misquoted-cn")]
#[test_case("missing-dtstamp")]
#[test_case("missing-dtstamp-email")]
#[test_case("missing-organizer")]
#[test_case("missing-organizer-email")]
#[test_case("missing-partstat")]
#[test_case("missing-sequence")]
#[test_case("multiple-exdates")]
#[test_case("multiple-exdates-tz")]
#[test_case("non-conformant-cn")]
#[test_case("nuku-alofa-tz")]
#[test_case("outside-uid")]
#[test_case("tabs")]
#[test_case("tentative-status")]
#[test_case("unexpected-newline-1")]
#[test_case("unexpected-newline-2")]
#[test_case("upper-case-status")]
#[test_case("whitespaces")]
fn atypical_case(name: &str) {
    let dir = Path::new("acceptance").join("atypical-cases").join(name);
    let src = fs::read(Path::new("tests").join(&dir).join("input.ics")).unwrap();
    let cal = VCalendar::from_bytes(&src).unwrap();

    let output = {
        let mut buf = String::new();

        _ = writeln!(buf, "```");
        _ = writeln!(buf, "{:#?}", cal.cal);
        _ = writeln!(buf, "```");

        if !cal.msgs.is_empty() {
            _ = writeln!(buf);
            _ = writeln!(buf, "# messages");

            for msg in cal.msgs {
                _ = writeln!(buf);
                _ = writeln!(buf, "{msg}");
            }
        }

        if !cal.viols.is_empty() {
            _ = writeln!(buf);
            _ = writeln!(buf, "# violations");

            for viol in cal.viols {
                _ = writeln!(buf);
                _ = writeln!(buf, "- {viol}");
            }
        }

        buf
    };

    let mut cfg = insta::Settings::clone_current();

    cfg.set_snapshot_path(&dir);
    cfg.set_omit_expression(true);
    cfg.set_prepend_module_to_snapshot(false);

    cfg.bind(|| {
        assert_snapshot!("output", output);
    });
}

#[track_caller]
fn assert(cal: &VCalendar, str: &str) {
    pa::assert_eq!(
        str.trim(),
        cal.validate().into_clean().unwrap().to_string().trim(),
        "VCalendar->String conversion returned a different string"
    );

    // ---

    let out = VCalendar::from_str(str).unwrap();

    pa::assert_eq!(
        cal,
        &out.cal,
        "String->VCalendar conversion returned a different object"
    );

    assert!(out.msgs.is_empty());
    assert!(out.viols.is_empty());
}
