use anyhow::{bail, Result};
use derive_more::From;
use std::env;
use std::process::Command;

/// Create a new version command
#[must_use]
pub fn version() -> Version {
    Version::default()
}

/// Create a new bench command
#[must_use]
pub fn bench() -> Bench {
    Bench::default()
}

/// Create a new build command
#[must_use]
pub fn build() -> Build {
    Build::default()
}

/// Create a new check command
#[must_use]
pub fn check() -> Check {
    Check::default()
}

/// Create a new clean command
#[must_use]
pub fn clean() -> Clean {
    Clean::default()
}

/// Create a new doc command
#[must_use]
pub fn doc() -> Doc {
    Doc::default()
}

/// Create a new run command
#[must_use]
pub fn run() -> Run {
    Run::default()
}

/// Create a new test command
#[must_use]
pub fn test() -> Test {
    Test::default()
}

/// Create a new add command
#[must_use]
pub fn add() -> Add {
    Add::default()
}

/// Create a new metadata command
#[must_use]
pub fn metadata() -> Metadata {
    Metadata::default()
}

/// Create a new update command
#[must_use]
pub fn update() -> Update {
    Update::default()
}

/// Create a new install command
#[must_use]
pub fn install(package: impl AsRef<str>) -> Install {
    Install::new(package)
}

/// Create a new package command
#[must_use]
pub fn package() -> Package {
    Package::default()
}

/// Create a new publish command
#[must_use]
pub fn publish() -> Publish {
    Publish::default()
}

/// A type that can be applied to a command.
trait Apply {
    fn apply(&self, cmd: &mut Command);
}

/// A helper for creating cargo commands.
#[derive(Debug)]
pub struct Cargo {
    /// The subcommand to invoke.
    cmd: CargoCmd,

    /// The options for the invoked `cargo` command.
    cargo: CargoOpt,

    /// The options for the invoked `rustc` command.
    rustc: RustcOpt,

    /// The options for the invoked `llvm` system.
    llvm: LlvmOpt,

    /// The options for the invoked `rustdoc` command.
    rustdoc: RustdocOpt,
}

impl<T> From<T> for Cargo
where
    T: Into<CargoCmd>,
{
    fn from(cmd: T) -> Self {
        Self {
            cmd: cmd.into(),
            cargo: CargoOpt::default(),
            rustc: RustcOpt::default(),
            llvm: LlvmOpt::default(),
            rustdoc: RustdocOpt::default(),
        }
    }
}

impl HasCargoOpt for Cargo {
    fn get_mut(&mut self) -> &mut CargoOpt {
        &mut self.cargo
    }
}

impl HasRustcOpt for Cargo {
    fn get_mut(&mut self) -> &mut RustcOpt {
        &mut self.rustc
    }
}

impl HasLlvmOpt for Cargo {
    fn get_mut(&mut self) -> &mut LlvmOpt {
        &mut self.llvm
    }
}

impl HasRustdocOpt for Cargo {
    fn get_mut(&mut self) -> &mut RustdocOpt {
        &mut self.rustdoc
    }
}

impl Apply for Cargo {
    fn apply(&self, cmd: &mut Command) {
        self.cmd.apply(cmd);
        self.cargo.apply(cmd);
        self.rustc.apply(cmd);
        self.llvm.apply(cmd);
        self.rustdoc.apply(cmd);
    }
}

/// An extension trait for `Cargo`.
pub trait CargoExt: Into<Cargo> + Sized {
    /// Converts the type into a `Cargo` command.
    fn into_cargo(self) -> Cargo {
        self.into()
    }

    /// Creates a new `Command` for the `cargo` executable.
    fn into_command(self) -> Command {
        let mut cmd = if let Ok(cross) = env::var("CROSS") {
            Command::new(cross)
        } else if let Ok(cargo) = env::var("CARGO") {
            Command::new(cargo)
        } else {
            Command::new("cargo")
        };

        self.into_cargo().apply(&mut cmd);

        cmd
    }

    /// Runs the command, returning an error if it failed.
    fn ok(self) -> Result<()> {
        match self.into_command().status()? {
            s if s.success() => Ok(()),
            s => bail!(s),
        }
    }

    /// Runs the command, returning the stdout.
    fn stdout(self) -> Result<String> {
        let out = match self.into_command().output()? {
            out if out.status.success() => out,
            out => bail!(out.status),
        };

        Ok(String::from_utf8(out.stdout)?)
    }
}

impl<T: Into<Cargo>> CargoExt for T {}

#[derive(Debug, From)]
enum CargoCmd {
    Version(Version),
    Bench(Bench),
    Build(Build),
    Check(Check),
    Clean(Clean),
    Doc(Doc),
    Run(Run),
    Test(Test),
    Add(Add),
    Metadata(Metadata),
    Update(Update),
    Install(Install),
    Package(Package),
    Publish(Publish),
}

impl Apply for CargoCmd {
    fn apply(&self, cmd: &mut Command) {
        match self {
            Self::Version(c) => c.apply(cmd),
            Self::Bench(c) => c.apply(cmd),
            Self::Build(c) => c.apply(cmd),
            Self::Check(c) => c.apply(cmd),
            Self::Clean(c) => c.apply(cmd),
            Self::Doc(c) => c.apply(cmd),
            Self::Run(c) => c.apply(cmd),
            Self::Test(c) => c.apply(cmd),
            Self::Add(c) => c.apply(cmd),
            Self::Metadata(c) => c.apply(cmd),
            Self::Update(c) => c.apply(cmd),
            Self::Install(c) => c.apply(cmd),
            Self::Package(c) => c.apply(cmd),
            Self::Publish(c) => c.apply(cmd),
        }
    }
}

#[derive(Debug, Default)]
struct CargoOpt {
    /// Whether to use unstable features.
    unstable: Option<Vec<String>>,

    /// The toolchain to use (e.g. 'nightly')
    toolchain: Option<String>,

    /// Whether to compile with incremental compilation.
    incremental: Option<bool>,

    /// Whether to override a registry URL.
    registry: Vec<(String, String)>,
}

/// A type that has cargo options.
trait HasCargoOpt {
    fn get_mut(&mut self) -> &mut CargoOpt;
}

/// An extension trait for `CargoOpt`.
pub trait CargoOptExt {
    /// Enables unstable features.
    fn cargo_unstable(self, feature: impl AsRef<str>) -> Self;

    /// Sets the toolchain to use.
    fn toolchain(self, toolchain: impl AsRef<str>) -> Self;

    /// Sets whether to compile with incremental compilation.
    fn incremental(self, incremental: bool) -> Self;

    /// Overrides a registry URL for the given package.
    fn registry(self, package: impl AsRef<str>, url: impl AsRef<str>) -> Self;
}

impl<T: HasCargoOpt> CargoOptExt for T {
    fn cargo_unstable(mut self, feature: impl AsRef<str>) -> Self {
        self.get_mut()
            .unstable
            .get_or_insert_with(Vec::new)
            .push(feature.as_ref().to_owned());

        self
    }

    fn toolchain(mut self, toolchain: impl AsRef<str>) -> Self {
        self.get_mut().toolchain = Some(toolchain.as_ref().to_owned());
        self
    }

    fn incremental(mut self, incremental: bool) -> Self {
        self.get_mut().incremental = Some(incremental);
        self
    }

    fn registry(mut self, reg: impl AsRef<str>, url: impl AsRef<str>) -> Self {
        let reg = reg.as_ref().to_uppercase().replace("-", "_");
        let url = url.as_ref().to_owned();

        self.get_mut().registry.push((reg, url));

        self
    }
}

impl Apply for CargoOpt {
    fn apply(&self, cmd: &mut Command) {
        if let Some(toolchain) = &self.toolchain {
            cmd.arg(format!("+{toolchain}"));
        }

        if let Some(unstable) = &self.unstable {
            for feature in unstable {
                cmd.arg("-Z").arg(feature);
            }
        }

        if let Some(inc) = self.incremental {
            cmd.env("CARGO_INCREMENTAL", if inc { "1" } else { "0" });
        }

        for (reg, url) in &self.registry {
            cmd.env(format!("CARGO_REGISTRIES_{reg}_INDEX"), url);
        }
    }
}

#[derive(Debug, Default)]
struct RustcOpt {
    unstable: Option<bool>,
    cfg: Vec<String>,
    cov: Option<bool>,
}

/// A type that has rustc options.
trait HasRustcOpt {
    fn get_mut(&mut self) -> &mut RustcOpt;
}

/// An extension trait for `RustcOpt`.
pub trait RustcOptExt {
    /// Enables unstable features.
    fn rustc_unstable(self, enable: bool) -> Self;

    /// Sets a configuration option.
    fn rustc_cfg(self, cfg: impl AsRef<str>) -> Self;

    /// Enables coverage instrumentation.
    fn rustc_coverage(self, enable: bool) -> Self;
}

impl<T: HasRustcOpt> RustcOptExt for T {
    fn rustc_unstable(mut self, enable: bool) -> Self {
        self.get_mut().unstable = Some(enable);
        self
    }

    fn rustc_cfg(mut self, cfg: impl AsRef<str>) -> Self {
        self.get_mut().cfg.push(cfg.as_ref().to_owned());
        self
    }

    fn rustc_coverage(mut self, enable: bool) -> Self {
        self.get_mut().cov = Some(enable);
        self
    }
}

impl Apply for RustcOpt {
    fn apply(&self, cmd: &mut Command) {
        let mut opt = Vec::new();

        if let Ok(cur) = env::var("RUSTFLAGS") {
            opt.push(cur);
        }

        if self.unstable.unwrap_or(false) {
            opt.push("-Z unstable-options".to_owned());
        }

        for cfg in &self.cfg {
            opt.push(format!("--cfg {cfg}"));
        }

        if self.cov.unwrap_or(false) {
            opt.push("--codegen instrument-coverage".to_owned());
        }

        cmd.env("RUSTFLAGS", opt.join(" "));
    }
}

#[derive(Debug, Default)]
struct LlvmOpt {
    profile_file: Option<String>,
}

/// A type that has llvm options.
trait HasLlvmOpt {
    fn get_mut(&mut self) -> &mut LlvmOpt;
}

/// An extension trait for `LlvmOpt`.
pub trait LlvmOptExt {
    /// Sets the profraw file (accepts a format string with '%p' and '%m').
    fn llvm_profraw(self, file: impl AsRef<str>) -> Self;
}

impl<T: HasLlvmOpt> LlvmOptExt for T {
    fn llvm_profraw(mut self, file: impl AsRef<str>) -> Self {
        self.get_mut().profile_file = Some(file.as_ref().to_owned());
        self
    }
}

impl Apply for LlvmOpt {
    fn apply(&self, cmd: &mut Command) {
        if let Some(file) = &self.profile_file {
            cmd.env("LLVM_PROFILE_FILE", file);
        }
    }
}

#[derive(Debug, Default)]
struct RustdocOpt {
    unstable: Option<bool>,
    index_page: Option<bool>,
}

/// A type that has rustdoc options.
trait HasRustdocOpt {
    fn get_mut(&mut self) -> &mut RustdocOpt;
}

/// An extension trait for `RustdocOpt`.
pub trait RustdocOptExt {
    /// Enables unstable features.
    fn rustdoc_unstable(self, enable: bool) -> Self;

    /// Enables the index page.
    fn rustdoc_enable_index_page(self, enable: bool) -> Self;
}

impl<T: HasRustdocOpt> RustdocOptExt for T {
    fn rustdoc_unstable(mut self, enable: bool) -> Self {
        self.get_mut().unstable = Some(enable);
        self
    }

    fn rustdoc_enable_index_page(mut self, enable: bool) -> Self {
        self.get_mut().index_page = Some(enable);
        self
    }
}

impl Apply for RustdocOpt {
    fn apply(&self, cmd: &mut Command) {
        let mut opt = Vec::new();

        if let Ok(cur) = env::var("RUSTDOCFLAGS") {
            opt.push(cur);
        }

        if self.unstable.unwrap_or(false) {
            opt.push("-Z unstable-options".to_owned());
        }

        if self.index_page.unwrap_or(false) {
            opt.push("--enable-index-page".to_owned());
        }

        cmd.env("RUSTDOCFLAGS", opt.join(" "));
    }
}

/// A `cargo version` command.
#[derive(Debug, Default)]
pub struct Version {}

impl Apply for Version {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("version");
    }
}

/// A `cargo bench` command.
#[derive(Debug, Default)]
pub struct Bench {
    opt: BuildOpt,
}

impl HasBuildOpt for Bench {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.opt
    }
}

impl Apply for Bench {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("bench");
        self.opt.apply(cmd);
    }
}

/// A `cargo build` command.
#[derive(Debug, Default)]
pub struct Build {
    opt: BuildOpt,
}

impl HasBuildOpt for Build {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.opt
    }
}

impl Apply for Build {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("build");
        self.opt.apply(cmd);
    }
}

/// A `cargo check` command.
#[derive(Debug, Default)]
pub struct Check {
    opt: BuildOpt,
}

impl HasBuildOpt for Check {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.opt
    }
}

impl Apply for Check {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("check");
        self.opt.apply(cmd);
    }
}

/// A `cargo clean` command.
#[derive(Debug, Default)]
pub struct Clean {
    opt: BuildOpt,
}

impl HasBuildOpt for Clean {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.opt
    }
}

impl Apply for Clean {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("clean");
        self.opt.apply(cmd);
    }
}

/// A `cargo doc` command.
#[derive(Debug, Default)]
pub struct Doc {
    no_deps: Option<bool>,
    private: Option<bool>,
    opt: BuildOpt,
}

impl Doc {
    /// Exclude external crates from the documentation.
    pub fn no_deps(mut self, enable: bool) -> Self {
        self.no_deps = Some(enable);
        self
    }

    /// Document private items.
    pub fn document_private_items(mut self, enable: bool) -> Self {
        self.private = Some(enable);
        self
    }
}

impl HasBuildOpt for Doc {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.opt
    }
}

impl Apply for Doc {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("doc");

        if self.no_deps.unwrap_or(false) {
            cmd.arg("--no-deps");
        }

        if self.private.unwrap_or(false) {
            cmd.arg("--document-private-items");
        }

        self.opt.apply(cmd);
    }
}

/// A `cargo run` command.
#[derive(Debug, Default)]
pub struct Run {
    opt: BuildOpt,
}

impl HasBuildOpt for Run {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.opt
    }
}

impl Apply for Run {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("run");
        self.opt.apply(cmd);
    }
}

/// A `cargo test` command.
#[derive(Debug, Default)]
pub struct Test {
    opt: BuildOpt,

    /// Number of threads to use for testing.
    threads: Option<usize>,
}

impl Test {
    /// Number of threads to use for testing.
    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads);
        self
    }
}

impl HasBuildOpt for Test {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.opt
    }
}

impl Apply for Test {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("test");

        if let Some(threads) = self.threads {
            cmd.env("RUST_TEST_THREADS", threads.to_string());
        }

        self.opt.apply(cmd);
    }
}

#[derive(Debug, Default)]
struct BuildOpt {
    /// Build all workspace members.
    workspace: Option<bool>,

    /// The package(s) to build.
    package: Vec<String>,

    /// Packages to exclude (when combined with `workspace`).
    exclude: Vec<String>,

    /// The triplet(s) to target.
    triplet: Vec<String>,

    /// The features to build.
    features: Vec<String>,

    /// Build all available features.
    all_features: Option<bool>,

    /// The target directory to use.
    target_dir: Option<String>,

    /// Build in release mode.
    release: Option<bool>,
}

/// A type that has build options.
trait HasBuildOpt {
    fn get_mut(&mut self) -> &mut BuildOpt;
}

/// An extension trait for `BuildOpt`.
pub trait BuildOptExt {
    /// Build all workspace members.
    fn workspace(self, workspace: bool) -> Self;

    /// The package(s) to build.
    fn package(self, package: impl IntoIterator<Item = impl AsRef<str>>) -> Self;

    /// Packages to exclude (when combined with `workspace`).
    fn exclude(self, exclude: impl IntoIterator<Item = impl AsRef<str>>) -> Self;

    /// The triplet(s) to target.
    fn triplet(self, triplet: impl IntoIterator<Item = impl AsRef<str>>) -> Self;

    /// The features to build.
    fn features(self, feature: impl IntoIterator<Item = impl AsRef<str>>) -> Self;

    /// Build all available features.
    fn all_features(self, all: bool) -> Self;

    /// Build in release mode.
    fn release(self, release: bool) -> Self;

    /// Sets the target directory to use.
    fn target_dir(self, dir: impl AsRef<str>) -> Self;
}

impl<T: HasBuildOpt> BuildOptExt for T {
    fn workspace(mut self, workspace: bool) -> Self {
        self.get_mut().workspace = Some(workspace);
        self
    }

    fn package(mut self, package: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for package in package {
            self.get_mut().package.push(package.as_ref().to_owned());
        }

        self
    }

    fn exclude(mut self, exclude: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for exclude in exclude {
            self.get_mut().exclude.push(exclude.as_ref().to_owned());
        }

        self
    }

    fn triplet(mut self, triplet: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for triplet in triplet {
            self.get_mut().triplet.push(triplet.as_ref().to_owned());
        }

        self
    }

    fn features(mut self, features: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for feature in features {
            self.get_mut().features.push(feature.as_ref().to_owned());
        }

        self
    }

    fn all_features(mut self, all: bool) -> Self {
        self.get_mut().all_features = Some(all);
        self
    }

    fn release(mut self, release: bool) -> Self {
        self.get_mut().release = Some(release);
        self
    }

    fn target_dir(mut self, dir: impl AsRef<str>) -> Self {
        self.get_mut().target_dir = Some(dir.as_ref().to_owned());
        self
    }
}

impl Apply for BuildOpt {
    fn apply(&self, cmd: &mut Command) {
        for package in &self.package {
            cmd.arg("--package").arg(package);
        }

        if self.workspace.unwrap_or(false) {
            cmd.arg("--workspace");
        }

        for exclude in &self.exclude {
            cmd.arg("--exclude").arg(exclude);
        }

        for target in &self.triplet {
            cmd.arg("--target").arg(target);
        }

        for feature in &self.features {
            cmd.arg("--features").arg(feature);
        }

        if self.all_features.unwrap_or(false) {
            cmd.arg("--all-features");
        }

        if self.release.unwrap_or(false) {
            cmd.arg("--release");
        }

        if let Some(dir) = &self.target_dir {
            cmd.arg("--target-dir").arg(dir);
        }
    }
}

/// A `cargo add` command.
#[derive(Debug, Default)]
pub struct Add {
    opt: ManifestOpt,
}

impl HasManifestOpt for Add {
    fn get_mut(&mut self) -> &mut ManifestOpt {
        &mut self.opt
    }
}

impl Apply for Add {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("add");
        self.opt.apply(cmd);
    }
}

/// A `cargo metadata` command.
#[derive(Debug, Default)]
pub struct Metadata {
    opt: ManifestOpt,

    /// Whether to exclude dependencies.
    no_deps: Option<bool>,
}

impl Metadata {
    /// Exclude dependencies.
    pub fn no_deps(mut self, enable: bool) -> Self {
        self.no_deps = Some(enable);
        self
    }
}

impl HasManifestOpt for Metadata {
    fn get_mut(&mut self) -> &mut ManifestOpt {
        &mut self.opt
    }
}

impl Apply for Metadata {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("metadata").args(["--format-version", "1"]);

        if self.no_deps.unwrap_or(false) {
            cmd.arg("--no-deps");
        }

        self.opt.apply(cmd);
    }
}

/// A `cargo update` command.
#[derive(Debug, Default)]
pub struct Update {
    opt: ManifestOpt,
}

impl HasManifestOpt for Update {
    fn get_mut(&mut self) -> &mut ManifestOpt {
        &mut self.opt
    }
}

impl Apply for Update {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("update");
        self.opt.apply(cmd);
    }
}

#[derive(Debug, Default)]
struct ManifestOpt {
    /// The path to the manifest file.
    manifest_path: Option<String>,
}

#[allow(unused)]
trait HasManifestOpt {
    fn get_mut(&mut self) -> &mut ManifestOpt;
}

/// An extension trait for `ManifestOpt`.
pub trait ManifestOptExt {
    /// Sets the path to the manifest file.
    fn manifest_path(self, path: impl AsRef<str>) -> Self;
}

impl<T: HasManifestOpt> ManifestOptExt for T {
    fn manifest_path(mut self, path: impl AsRef<str>) -> Self {
        self.get_mut().manifest_path = Some(path.as_ref().to_owned());
        self
    }
}

impl Apply for ManifestOpt {
    fn apply(&self, cmd: &mut Command) {
        if let Some(path) = &self.manifest_path {
            cmd.arg("--manifest-path").arg(path);
        }
    }
}

/// A `cargo install` command.
#[derive(Debug)]
pub struct Install {
    package: String,
    opt: PackageOpt,
}

impl Install {
    /// Create a new install command.
    pub fn new(package: impl AsRef<str>) -> Self {
        Self {
            package: package.as_ref().to_owned(),
            opt: PackageOpt::default(),
        }
    }
}

impl HasPackageOpt for Install {
    fn get_mut(&mut self) -> &mut PackageOpt {
        &mut self.opt
    }
}

impl Apply for Install {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("install");
        cmd.arg(&self.package);

        self.opt.apply(cmd);
    }
}

#[derive(Debug, Default)]
struct PackageOpt {}

/// A type that has package options.
#[allow(unused)]
trait HasPackageOpt {
    fn get_mut(&mut self) -> &mut PackageOpt;
}

/// An extension trait for `PackageOpt`.
pub trait PackageOptExt {}

impl<T: HasPackageOpt> PackageOptExt for T {}

impl Apply for PackageOpt {
    fn apply(&self, _: &mut Command) {}
}

/// A `cargo package` command.
#[derive(Debug, Default)]
pub struct Package {
    build_opt: BuildOpt,
    manifest_opt: ManifestOpt,
    publish_opt: PublishOpt,

    /// Index of registry to use.
    registry: Option<String>,
}

impl Package {
    /// The registry to use.
    pub fn registry(mut self, name: impl AsRef<str>) -> Self {
        self.registry = Some(name.as_ref().to_owned());
        self
    }
}

impl HasBuildOpt for Package {
    fn get_mut(&mut self) -> &mut BuildOpt {
        &mut self.build_opt
    }
}

impl HasManifestOpt for Package {
    fn get_mut(&mut self) -> &mut ManifestOpt {
        &mut self.manifest_opt
    }
}

impl HasPublishOpt for Package {
    fn get_mut(&mut self) -> &mut PublishOpt {
        &mut self.publish_opt
    }
}

impl Apply for Package {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("package");

        if let Some(name) = &self.registry {
            cmd.arg("--registry").arg(name);
        }

        self.build_opt.apply(cmd);
        self.manifest_opt.apply(cmd);
        self.publish_opt.apply(cmd);
    }
}

/// A `cargo publish` command.
#[derive(Debug, Default)]
pub struct Publish {
    opt: PublishOpt,
}

impl HasPublishOpt for Publish {
    fn get_mut(&mut self) -> &mut PublishOpt {
        &mut self.opt
    }
}

impl Apply for Publish {
    fn apply(&self, cmd: &mut Command) {
        cmd.arg("publish");
        self.opt.apply(cmd);
    }
}

#[derive(Debug, Default)]
struct PublishOpt {}

/// A type that has publish options.
#[allow(unused)]
trait HasPublishOpt {
    fn get_mut(&mut self) -> &mut PublishOpt;
}

/// An extension trait for `PublishOpt`.
pub trait PublishOptExt {}

impl<T: HasPublishOpt> PublishOptExt for T {}

impl Apply for PublishOpt {
    fn apply(&self, _: &mut Command) {}
}
