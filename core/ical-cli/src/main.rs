#![allow(clippy::print_stdout)]

mod check;
mod convert;
mod print;

use self::check::CheckCmd;
use self::convert::ConvertCmd;
use self::print::PrintCmd;
use anyhow::Result;
use clap::Parser;

#[derive(Clone, Debug, Parser)]
enum Cmd {
    Check(CheckCmd),
    Convert(ConvertCmd),
    Print(PrintCmd),
}

fn main() -> Result<()> {
    match Cmd::parse() {
        Cmd::Check(cmd) => cmd.run(),
        Cmd::Convert(cmd) => cmd.run(),
        Cmd::Print(cmd) => cmd.run(),
    }
}
