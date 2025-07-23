#![no_main]

#[macro_use]
extern crate libfuzzer_sys;
extern crate proton_ical as ical;

fuzz_target!(|data: &[u8]| {
    _ = ical::VCalendar::from_bytes(data);
});
