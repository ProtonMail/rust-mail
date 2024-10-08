use crate::cargo::{BuildOptExt, CargoExt, CargoOptExt, LlvmOptExt, RustcOptExt};
use crate::{cargo, run};
use anyhow::Result;
use cargo_metadata::camino::Utf8Path;
use cargo_metadata::Metadata;
use clap::Args;

#[derive(Debug, Args)]
pub struct Test {
    /// Number of threads to use for testing.
    #[arg(long)]
    threads: Option<usize>,
}

impl Test {
    #[allow(dead_code)]
    pub fn run(self, meta: Metadata) -> Result<()> {
        let src = &meta.workspace_root;
        let tgt = &meta.target_directory;

        let prof = tgt.join("prof");
        let lcov = tgt.join("lcov.info");
        let html = tgt.join("coverage");

        self.run_test(&prof)?;
        run_grcov(src, tgt, &prof, &lcov)?;
        run_genhtml(src, &lcov, &html)?;

        Ok(())
    }

    #[allow(dead_code)]
    fn run_test(&self, prof: &Utf8Path) -> Result<()> {
        cargo::test()
            .all_features(true)
            .threads(self.threads.unwrap_or_else(num_cpus::get))
            .into_cargo()
            .incremental(false)
            .rustc_coverage(true)
            .llvm_profraw(prof.join("%p-%m.profraw"))
            .ok()?;

        Ok(())
    }
}

#[allow(dead_code)]
fn run_grcov(src: &Utf8Path, tgt: &Utf8Path, prof: &Utf8Path, lcov: &Utf8Path) -> Result<()> {
    cargo::install("grcov").ok()?;

    run! {
        "grcov", prof,
        "-t", "lcov",
        "-s", src,
        "-b", tgt,
        "-o", lcov,
        "--keep-only", "muon/*",
        "--keep-only", "muon-*/*",
        "--ignore-not-existing",
    }?;

    Ok(())
}

#[allow(dead_code)]
fn run_genhtml(src: &Utf8Path, lcov: &Utf8Path, html: &Utf8Path) -> Result<()> {
    run!("genhtml", lcov, "-p", src, "-o", html)?;

    Ok(())
}
