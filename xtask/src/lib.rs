//! This project uses the xtask pattern to define custom cargo commands.
//!
//! See <https://github.com/matklad/cargo-xtask>.

#[macro_use]
extern crate tracing;

use crate::doc::Doc;
use crate::generate::Generate;
use crate::publish::Publish;
use crate::test::Test;
use anyhow::Result;
use cargo_metadata::MetadataCommand;
use clap::Parser;

mod doc;
mod generate;
mod publish;
mod registry;
mod test;
mod util;

/// Cargo wrapper.
pub mod cargo;

/// The xtask CLI.
#[derive(Debug, Parser)]
pub enum Cli {
    /// Generate the documentation.
    Doc(Doc),

    /// Generate the FFI bindings.
    Generate(Generate),

    /// Publish the crates.
    Publish(Publish),

    /// Run the tests.
    Test(Test),
}

impl Cli {
    /// Run the CLI.
    pub fn run(self) -> Result<()> {
        let meta = MetadataCommand::new().exec()?;

        match self {
            Self::Doc(cmd) => cmd.run(meta),
            Self::Generate(cmd) => cmd.run(meta),
            Self::Publish(cmd) => cmd.run(meta),
            Self::Test(cmd) => cmd.run(meta),
        }
    }
}
