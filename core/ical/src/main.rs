#![allow(clippy::print_stdout)]

use proton_ical::VCalendar;
use std::{env, fs};

fn main() {
    let path = env::args().nth(1).expect("usage:\nical <path>");

    let src = fs::read(&path).unwrap_or_else(|err| {
        panic!("couldn't read `{path}`: {err}");
    });

    let out = VCalendar::from_bytes(&src).unwrap_or_else(|err| {
        panic!("couldn't parse `{path}`: {err:?}");
    });

    if !out.msgs.is_empty() {
        for msg in out.msgs {
            println!("{}", msg.to_string(&*src));
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
}
