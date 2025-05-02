#![allow(clippy::print_stdout)]

use std::{env, fs};

fn main() {
    let path = env::args().nth(1).expect("usage:\nical <path>");

    let src = fs::read(&path).unwrap_or_else(|err| {
        panic!("couldn't read `{path}`: {err}");
    });

    let (cal, msgs) = ical::VCalendar::from_bytes(&src).unwrap();

    if !msgs.is_empty() {
        for msg in msgs {
            println!("{}", msg.to_string(&*src));
        }
    }

    println!("{cal:#?}");
}
