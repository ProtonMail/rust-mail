//! The xtask CLI entry point.

use anyhow::Result;
use clap::Parser;
use xtask::Cli;

/// Run the parsed xtask command.
fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    Cli::parse().run()
}
