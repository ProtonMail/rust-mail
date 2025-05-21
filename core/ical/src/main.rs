#![allow(clippy::print_stdout)]

use proton_ical::{VCalendar, ValidatedVCalendar};
use std::{env, fs};

fn main() {
    let src_path = env::args().nth(1).expect("usage:\nical <src> [dst]");
    let dst_path = env::args().nth(2);

    let src = fs::read(&src_path).unwrap_or_else(|err| {
        panic!("couldn't read `{src_path}`: {err}");
    });

    let out = VCalendar::from_bytes(&src).unwrap_or_else(|err| {
        panic!("couldn't parse `{src_path}`: {err}");
    });

    if !out.msgs.is_empty() {
        for msg in out.msgs {
            println!("{msg}");
        }

        println!();
    }

    if !out.viols.is_empty() {
        for viol in out.viols {
            println!("{viol}");
        }

        println!();
    }

    println!("{:#?}", out.cal);

    if let Some(dst_path) = dst_path {
        let cal = match out.cal.validate() {
            ValidatedVCalendar::Clean(cal) => cal.to_string(),
            ValidatedVCalendar::Dirty(cal) => cal.to_string(),
        };

        fs::write(&dst_path, &cal).unwrap_or_else(|err| {
            panic!("couldn't write `{dst_path}`: {err}");
        });
    }
}
