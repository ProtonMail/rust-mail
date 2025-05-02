<?php

$str = ical_sanitize(
    <<<ICAL
    BEGIN:VCALENDAR
    PRODID:-//Proton AG//iCal//EN
    VERSION:2.0
    CALSCALE:GREGORIAN
    BEGIN:VEVENT
    UID:0000-0000-0000-0001
    DTSTAMP:20240101T120000Z
    DTSTART:20240101T100000Z
    CLASS:CONFIDENTIAL
    TRANSP:TRANSPARENT
    STATUS:CONFIRMED
    PRIORITY:9
    RRULE:FREQ=MONTHLY;BYDAY=MO,-1TH,2FR
    EXDATE:20180108T100000Z,20180115T100000Z
    DURATION:PT2H45M
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

$result = ical_parse($str);

assert(empty($result->messages));

$str2 = ical_print($result->calendar);

assert(
    $str === $str2,
    "\n\nical_print() returned a string that's different from the initial one\n"
    . "\n"
    . "<input-string>\n"
    . "{$str}\n"
    . "</input-string>\n"
    . "\n"
    . "<output-string>\n"
    . "{$str2}\n"
    . "</output-string>\n"
);

echo "ok";
