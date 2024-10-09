use crate::cargo::{self, BuildOptExt, CargoExt, CargoOptExt, ManifestOptExt};
use crate::registry::Registry;
use anyhow::Result;
use cargo_metadata::Metadata;
use clap::Args;

#[derive(Debug, Args)]
pub struct Publish {
    /// The registry name.
    #[arg(long)]
    registry: String,

    /// The registry repository.
    #[arg(long)]
    repository: String,

    /// The packages to exclude from publishing.
    #[arg(long)]
    exclude: Vec<String>,

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
        let registry = Registry::new(
            &self.repository,
            &self.name,
            &self.email,
            &meta.target_directory,
        )?;

        let mut include = Vec::new();
        let mut exclude = Vec::new();

        for pkg in meta.workspace_packages() {
            if registry.has(&pkg.name, &pkg.version)? {
                exclude.push(&pkg.name);
            } else if self.exclude.contains(&pkg.name) {
                exclude.push(&pkg.name);
            } else {
                include.push(pkg);
            }
        }

        cargo::package()
            .workspace(true)
            .registry(self.registry)
            .exclude(exclude)
            .into_cargo()
            .cargo_unstable("package-workspace")
            .ok()?;

        for pkg in include {
            let data = (meta.target_directory)
                .join("package")
                .join(format!("{}-{}.crate", &pkg.name, &pkg.version));

            let json = cargo::metadata()
                .manifest_path(&pkg.manifest_path)
                .stdout()?;

            registry.commit(&pkg.name, &pkg.version, &data, &json)?;
        }

        if !self.dry_run {
            registry.push()?;
        } else {
            info!("skipping push due to dry run");
        }

        Ok(())
    }
}
