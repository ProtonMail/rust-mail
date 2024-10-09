use crate::cargo::{self, CargoExt, CargoOptExt, ManifestOptExt};
use crate::registry::Registry;
use anyhow::Result;
use cargo_metadata::Metadata;
use clap::Args;

#[derive(Debug, Args)]
pub struct Publish {
    /// The registry name.
    #[arg(long)]
    reg_name: String,

    /// The registry repository.
    #[arg(long)]
    reg_repo: String,

    /// The username to commit as.
    #[arg(long)]
    name: String,

    /// The email to commit as.
    #[arg(long)]
    email: String,

    /// Whether to perform a dry run.
    #[arg(long)]
    dry_run: bool,
}

impl Publish {
    pub fn run(self, meta: Metadata) -> Result<()> {
        let reg = Registry::new(
            &self.reg_repo,
            &self.name,
            &self.email,
            &meta.target_directory,
        )?;

        cargo::package()
            .workspace(true)
            .registry(self.reg_name)
            .into_cargo()
            .cargo_unstable("package-workspace")
            .ok()?;

        for pkg in meta.workspace_packages() {
            let data = meta
                .target_directory
                .join("package")
                .join(format!("{}-{}.crate", &pkg.name, &pkg.version));

            let json = cargo::metadata()
                .manifest_path(&pkg.manifest_path)
                .stdout()?;

            reg.commit(&pkg.name, &pkg.version, &data, &json)?;
        }

        if !self.dry_run {
            reg.push()?;
        } else {
            info!("skipping push due to dry run");
        }

        Ok(())
    }
}
