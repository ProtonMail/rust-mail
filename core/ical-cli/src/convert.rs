use anyhow::{Context, Result, anyhow};
use clap::Parser;
use proton_ical::{VCalendar, ValidatedVCalendar};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Parser)]
pub struct ConvertCmd {
    src: PathBuf,
    dst: PathBuf,
}

impl ConvertCmd {
    pub fn run(self) -> Result<()> {
        if self.src.is_dir() {
            return Err(anyhow!("source path must be a file"));
        }

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

        let str = match out.cal.validate() {
            ValidatedVCalendar::Clean(cal) => cal.to_string(),

            ValidatedVCalendar::Dirty(cal) => {
                eprintln!(
                    "warn: since the source *.ics is mildly illegal, the \
                     returned *.ics might not be compatible with all clients",
                );

                cal.to_string()
            }
        };

        fs::write(&self.dst, &str)
            .with_context(|| format!("couldn't write `{}`", self.dst.display()))?;

        Ok(())
    }
}
