#![allow(dead_code)]
use crate::cargo;
use crate::cargo::{BuildOptExt, CargoExt};
use anyhow::{bail, Result};
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::Metadata;
use clap::{Args, ValueEnum};
use derive_more::Display;
use serde::Deserialize;
use std::env::consts::{DLL_EXTENSION, DLL_PREFIX};

#[derive(Debug, Args)]
pub struct Generate {
    /// The language(s) to generate bindings for.
    #[arg(long, value_enum)]
    language: Vec<Language>,
}

#[derive(Debug, Display, Clone, Copy, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[display("python")]
    Python,

    #[display("kotlin")]
    Kotlin,

    #[display("swift")]
    Swift,
}

impl Generate {
    pub fn run(self, meta: Metadata) -> Result<()> {
        let lib = run_build(&meta.target_directory)?;
        let ffi = meta.target_directory.join("ffi");

        for lang in self.language {
            run_uniffi(lang, &lib, &ffi)?;
        }

        Ok(())
    }
}

fn run_build(tgt: &Utf8Path) -> Result<Utf8PathBuf> {
    cargo::build().package(["muon-ffi"]).release(true).ok()?;

    let lib = tgt
        .join("release")
        .join(format!("{DLL_PREFIX}muon_ffi"))
        .with_extension(DLL_EXTENSION);

    if lib.exists() {
        Ok(lib)
    } else {
        bail!("failed to build the FFI library");
    }
}

fn run_uniffi(lang: Language, lib: &Utf8Path, out: &Utf8Path) -> Result<()> {
    let status = cargo::run()
        .package(["muon-ffi"])
        .into_command()
        .arg("generate")
        .args(["--language", lang.to_string().as_str()])
        .args(["--library", lib.as_str()])
        .args(["--out-dir", out.as_str()])
        .status()?;

    if status.success() {
        Ok(())
    } else {
        bail!("failed to generate bindings for {lang}")
    }
}
