<?php

$result = ical_parse(
    <<<ICAL
    BEGIN:VCALENDAR
    PRODID:-//Proton AG//iCal//EN
    VERSION:2
    BEGIN:VEVENT
    UID:0000-0000-0000-0001
    DTSTAMP;TZID=Europe/Stockholm:20240101T120000
    DTSTART;TZID=Europe/Stockholm:20240101T100000
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
    ICAL
);

foreach ($result->messages as $msg) {
    echo "{$msg->text}\n";
}

var_dump($result->calendar);
