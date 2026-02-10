use anyhow::{Context, Result};
use clap::Parser;
use proton_ical::VCalendar;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fmt, fs};

#[derive(Clone, Debug, Parser)]
pub struct CheckCmd {
    src: PathBuf,
}

impl CheckCmd {
    pub fn run(self) -> Result<()> {
        if self.src.is_dir() {
            self.run_on_dir()?;
        } else {
            self.run_on_file()?;
        }

        Ok(())
    }

    fn run_on_dir(self) -> Result<()> {
        let entries = fs::read_dir(&self.src)
            .with_context(|| format!("couldn't read `{}`", self.src.display()))?;

        let mut outcomes = BTreeMap::<_, Vec<_>>::new();

        for entry in entries {
            let path = entry?.path();

            if path
                .extension()
                .is_none_or(|ext| ext.to_str() != Some("ics"))
            {
                continue;
            }

            println!("# {}", path.display());
            println!();

            let outcome = Self::check(&path)?;

            outcomes.entry(outcome).or_default().push(path);

            println!();
        }

        println!("# summary");
        println!();

        if outcomes.is_empty() {
            println!("found no *.ics files");
        } else {
            for (outcome, files) in &outcomes {
                println!("- {outcome}: {}", files.len());
            }

            for (outcome, files) in outcomes {
                println!();
                println!("## {outcome}");
                println!();

                for file in files {
                    println!("- {}", file.display());
                }
            }
        }

        Ok(())
    }

    fn run_on_file(self) -> Result<()> {
        Self::check(&self.src)?;

        Ok(())
    }

    fn check(path: &Path) -> Result<CheckOutcome> {
        let src = fs::read(path).with_context(|| format!("couldn't read `{}`", path.display()))?;

        match VCalendar::from_bytes(&src) {
            Ok(out) => {
                if out.msgs.is_empty() && out.viols.is_empty() {
                    println!("ok, spotless");

                    Ok(CheckOutcome::Spotless)
                } else {
                    let has_errors = out.msgs.iter().any(|msg| msg.kind.is_error());

                    if has_errors {
                        println!("meh, dubious:");
                    } else {
                        println!("ok, with some remarks:");
                    }

                    for msg in &out.msgs {
                        println!();
                        println!("{msg}");
                    }

                    for viol in &out.viols {
                        println!();
                        println!("{viol}");
                    }

                    if has_errors {
                        Ok(CheckOutcome::Dubious)
                    } else {
                        Ok(CheckOutcome::GoodEnough)
                    }
                }
            }

            Err(err) => {
                println!("{err}");

                Ok(CheckOutcome::Invalid)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CheckOutcome {
    /// *.ics raised no errors or warnings (a unicorn!).
    Spotless,

    /// *.ics raised some parser or validator warnings, but we were able to
    /// parse the entire file anyway.
    GoodEnough,

    /// *.ics contained some syntax errors, but we were able to recover most
    /// (or at least some) of the content.
    Dubious,

    /// *.ics couldn't have been parsed at all.
    Invalid,
}

impl fmt::Display for CheckOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckOutcome::Spotless => write!(f, "spotless"),
            CheckOutcome::GoodEnough => write!(f, "good-enough"),
            CheckOutcome::Dubious => write!(f, "dubious"),
            CheckOutcome::Invalid => write!(f, "invalid"),
        }
    }
}
