use crate::values::date::{
    is_date_complete_value, is_date_noreduc_value, is_day_value, is_month_and_day_value,
    is_month_value, is_year_value,
};
use crate::values::date_time::is_date_time_value;
use crate::values::time::{
    is_hour_value, is_minute_value, is_second_value, is_time_complete_value, is_time_notrunc_value,
    is_time_value,
};
use crate::values::utc_offset::is_utc_offset_value;
use crate::values::zone::is_zone_value;

#[test]
fn utc_offset_value() {
    assert!(is_utc_offset_value("+01"));
    assert!(is_utc_offset_value("-0123"));
    assert!(!is_utc_offset_value(""));
    assert!(!is_utc_offset_value("01"));
    assert!(!is_utc_offset_value("+012"));
}

#[test]
fn time_value() {
    assert!(is_time_value("01"));
    assert!(is_time_value("0123"));
    assert!(is_time_value("012345"));
    assert!(is_time_value("01Z"));
    assert!(is_time_value("0123+01"));
    assert!(is_time_value("012345-0123"));
    assert!(is_time_value("-23"));
    assert!(is_time_value("-2345"));
    assert!(is_time_value("-23Z"));
    assert!(is_time_value("-2345+01"));
    assert!(is_time_value("--45"));
    assert!(is_time_value("--45Z"));
    assert!(!is_time_value(""));
    assert!(!is_time_value("foo"));
}

#[test]
fn hour_value() {
    assert!(is_hour_value("23"));
    assert!(!is_hour_value(""));
    assert!(!is_hour_value("1"));
    assert!(!is_hour_value("123"));
    assert!(!is_hour_value("24"));
}

#[test]
fn minute_value() {
    assert!(is_minute_value("59"));
    assert!(!is_minute_value("60"));
    assert!(!is_minute_value(""));
    assert!(!is_minute_value("1"));
}

#[test]
fn second_value() {
    assert!(is_second_value("60"));
    assert!(!is_second_value("61"));
    assert!(!is_second_value(""));
    assert!(!is_second_value("1"));
}

#[test]
fn date_time_value() {
    assert!(is_date_time_value("19961022T140000"));
    assert!(is_date_time_value("--1022T1400"));
    assert!(is_date_time_value("---01T10"));
    assert!(!is_date_time_value(""));
    assert!(!is_date_time_value("---0110"));
}

#[test]
fn date_complete_value() {
    assert!(is_date_complete_value("20140614"));
    assert!(!is_date_complete_value("2014061"));
    assert!(!is_date_complete_value("201406145"));
    assert!(!is_date_complete_value("abcdefgh"));
}

#[test]
fn time_complete_value() {
    assert!(is_time_complete_value("235959"));
    assert!(is_time_complete_value("235959+0123"));
    assert!(!is_time_complete_value(""));
    assert!(!is_time_complete_value("foo"));
}

#[test]
fn date_noreduc_value() {
    assert!(is_date_noreduc_value("20140614"));
    assert!(is_date_noreduc_value("--0614"));
    assert!(is_date_noreduc_value("---14"));
    assert!(!is_date_noreduc_value(""));
    assert!(!is_date_noreduc_value("foo"));
}

#[test]
fn time_notrunc_value() {
    assert!(is_time_notrunc_value("23"));
    assert!(is_time_notrunc_value("2359"));
    assert!(is_time_notrunc_value("235959"));
    assert!(is_time_notrunc_value("23Z"));
    assert!(is_time_notrunc_value("2359+01"));
    assert!(is_time_notrunc_value("235959+0123"));
    assert!(!is_time_notrunc_value(""));
    assert!(!is_time_notrunc_value("foo"));
}

#[test]
fn year_value() {
    assert!(is_year_value("0000"));
    assert!(is_year_value("9999"));
    assert!(!is_year_value("0"));
    assert!(!is_year_value(""));
    assert!(!is_year_value("foo"));
}

#[test]
fn month_value() {
    assert!(is_month_value("01"));
    assert!(is_month_value("12"));
    assert!(!is_month_value("1"));
    assert!(!is_month_value("13"));
    assert!(!is_month_value(""));
    assert!(!is_month_value("foo"));
}

#[test]
fn day_value() {
    assert!(is_day_value("01"));
    assert!(is_day_value("31"));
    assert!(!is_day_value("00"));
    assert!(!is_day_value("1"));
    assert!(!is_day_value(""));
    assert!(!is_day_value("foo"));
}

#[test]
fn month_an_day_value() {
    assert!(is_month_and_day_value("0101"));
    assert!(is_month_and_day_value("1231"));
    assert!(!is_month_and_day_value("0001"));
    assert!(!is_month_and_day_value("0100"));
    assert!(!is_month_and_day_value("1301"));
    assert!(!is_month_and_day_value("1232"));
    assert!(!is_month_and_day_value("0230"));
    assert!(!is_month_and_day_value("0431"));
    assert!(!is_month_and_day_value("0631"));
    assert!(!is_month_and_day_value("0931"));
    assert!(!is_month_and_day_value("1131"));
    assert!(!is_month_and_day_value("111"));
    assert!(!is_month_and_day_value("11111"));
    assert!(!is_month_and_day_value(""));
    assert!(!is_month_and_day_value("fooo"));
}

#[test]
fn zone_value() {
    assert!(is_zone_value("Z"));
    assert!(is_zone_value("+01"));
    assert!(is_zone_value("-0123"));
    assert!(!is_zone_value(""));
    assert!(!is_zone_value("foo"));
}
