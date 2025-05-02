#![allow(clippy::print_stdout)]

use ical::ParsedVCalendar;
use std::{env, fs};

fn main() {
    let path = env::args().nth(1).expect("usage:\nical <path>");

    let src = fs::read(&path).unwrap_or_else(|err| {
        panic!("couldn't read `{path}`: {err}");
    });

    let ParsedVCalendar { cal, msgs, viols } =
        ical::VCalendar::from_bytes(&src).unwrap_or_else(|err| {
            panic!("couldn't parse `{path}`: {err:?}");
        });

    if !msgs.is_empty() {
        for msg in msgs {
            println!("{}", msg.to_string(&*src));
        }

        println!();
    }

    if !viols.is_empty() {
        for viol in viols {
            println!("{viol}");
        }

        println!();
    }

    println!("{cal:#?}");
}
