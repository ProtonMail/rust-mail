use crate::cargo;
use crate::cargo::{BuildOptExt, CargoExt, RustdocOptExt};
use anyhow::Result;
use cargo_metadata::Metadata;
use clap::Args;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DocCfg {}

#[derive(Debug, Args)]
pub struct Doc {}

impl Doc {
    #[allow(dead_code)]
    pub fn run(self, _: Metadata) -> Result<()> {
        cargo::doc()
            .workspace(true)
            .all_features(true)
            .document_private_items(true)
            .no_deps(true)
            .into_cargo()
            .rustdoc_unstable(true)
            .rustdoc_enable_index_page(true)
            .ok()?;

        Ok(())
    }
}
