use ical::*;
use pretty_assertions as pa;

#[test]
fn empty() {
    let cal = VCalendar::new("-//Proton AG//test 2.1.3.7//EN");

    let str = ical! {"
        BEGIN:VCALENDAR
        PRODID:-//Proton AG//test 2.1.3.7//EN
        VERSION:2.0
        CALSCALE:GREGORIAN
        END:VCALENDAR
    "};

    assert(&cal, &str);
}

#[test]
fn with_event() {
    let cal = VCalendar::new("-//Proton AG//test 2.1.3.7//EN").with_event(
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

    let str = ical! {"
        BEGIN:VCALENDAR
        PRODID:-//Proton AG//test 2.1.3.7//EN
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
fn with_broken_event() {
    let str = ical! {"
        BEGIN:VCALENDAR
        PRODID:-//Proton AG//test 2.1.3.7//EN
        VERSION:2.0
        CALSCALE:GREGORIAN
        BEGIN:VEVENT
        DTSTART:20240101T100000
        RRULE:FREQ=DAILY;COUNT=5
        END:VEVENT
        END:VCALENDAR
    "};

    let ParsedVCalendar { cal, msgs, viols } = VCalendar::from_str(&str).unwrap();

    // ---
    // Assert calendar

    assert_eq!(1, cal.events.len());

    // ---
    // Assert messages

    let actual: Vec<_> = msgs.into_iter().map(|msg| msg.to_string(None)).collect();

    let expected = vec![
        "error: missing property `UID`",
        "error: missing property `DTSTAMP`",
    ];

    assert_eq!(actual, expected);

    // ---
    // Assert violations

    let actual = viols
        .into_iter()
        .map(|viol| viol.to_string())
        .collect::<Vec<_>>();

    let expected = vec!["event[0]: uid is missing", "event[0]: dtstamp is missing"];

    assert_eq!(actual, expected);
}

#[test]
fn with_method() {
    let cal = VCalendar::new("-//Proton AG//test 2.1.3.7//EN").with_method(Method::Publish);

    let str = ical! {"
        BEGIN:VCALENDAR
        METHOD:PUBLISH
        PRODID:-//Proton AG//test 2.1.3.7//EN
        VERSION:2.0
        CALSCALE:GREGORIAN
        END:VCALENDAR
    "};

    assert(&cal, &str);
}

#[test]
fn without_calscale() {
    let str = ical! {"
        BEGIN:VCALENDAR
        PRODID:-//Proton AG//test 2.1.3.7//EN
        VERSION:2.0
        END:VCALENDAR
    "};

    let ParsedVCalendar { cal, msgs, viols } = VCalendar::from_str(&str).unwrap();

    assert_eq!(CalScale::Gregorian, cal.calscale);
    assert!(msgs.is_empty());
    assert!(viols.is_empty());
}

#[track_caller]
fn assert(cal: &VCalendar, str: &str) {
    // ---
    // Convert VCalendar to string

    pa::assert_eq!(
        str.trim(),
        cal.validate().into_clean().unwrap().to_string().trim(),
        "VCalendar->String assertion failed"
    );

    // ---
    // Convert string to VCalendar

    let ParsedVCalendar {
        cal: cal2,
        msgs,
        viols,
    } = VCalendar::from_str(str).unwrap();

    pa::assert_eq!(cal, &cal2, "String->VCalendar assertion failed");
    assert!(msgs.is_empty());
    assert!(viols.is_empty());
}
