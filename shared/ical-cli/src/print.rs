use anyhow::{Context, Result};
use clap::Parser;
use proton_ical::VCalendar;
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Parser)]
pub struct PrintCmd {
    src: PathBuf,
}

impl PrintCmd {
    pub fn run(self) -> Result<()> {
        let src = fs::read(&self.src)
            .with_context(|| format!("couldn't read `{}`", self.src.display()))?;

        let out = VCalendar::from_bytes(&src)
            .with_context(|| format!("couldn't parse `{}`", self.src.display()))?;

        for msg in &out.msgs {
            eprintln!("{msg}");
            eprintln!();
        }

        for viol in &out.viols {
            eprintln!("{viol}");
            eprintln!();
        }

        println!("{:#?}", out.cal);

        Ok(())
    }
}
