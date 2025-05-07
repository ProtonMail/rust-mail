<?php

$cal = ical_new('//Proton AG//test//EN');

$cal->events[] = ical_new_event(
    '0000-0000-0000-0001',
    new DateTimeImmutable('2018-01-01 12:00:00'),
);

echo ical_print($cal);
