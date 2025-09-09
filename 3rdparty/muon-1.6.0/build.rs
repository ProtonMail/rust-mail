//! Build script to compile and link the Go crypto library.
//!
//! This invokes the Go compiler to build the Go library,
//! and then configures the Rust compiler to link against it.

use anyhow::{bail, Result};
use camino::{Utf8Path, Utf8PathBuf};
use derive_more::Display;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::env::consts::DLL_EXTENSION;
use std::io;
use std::io::Write;

lazy_static! {
    static ref TARGET_FAMILY: String = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap();
    static ref CWD: Utf8PathBuf = std::env::var("CARGO_MANIFEST_DIR").unwrap().into();
    static ref OUT: Utf8PathBuf = std::env::var("OUT_DIR").unwrap().into();
}

fn main() -> Result<()> {
    if TARGET_FAMILY.ne("wasm") {
        if cfg!(feature = "testing-crypto") {
            build_crypto()?;
        } else {
            eprintln!("skipping Go crypto library build");
        }
    } else {
        eprintln!("skipping Go crypto library build for WebAssembly target");
    }

    Ok(())
}

fn build_crypto() -> Result<()> {
    let mode = if cfg!(crypto_dll) {
        BuildMode::Dynamic
    } else {
        BuildMode::Static
    };

    let vcs = if cfg!(crypto_vcs) {
        BuildVcs::True
    } else {
        BuildVcs::False
    };

    go_build(&CWD.join("extern").join("go-crypto"), &OUT, mode, vcs)?;

    Ok(())
}

/// The way to link the Go library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildMode {
    Static,
    Dynamic,
}

/// Whether to stamp the VCS info into the build.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
enum BuildVcs {
    #[display("true")]
    True,

    #[display("false")]
    False,
}

/// Build the Go package at the given path.
///
/// This will build the Go package and generate Rust bindings.
/// The bindings will be written to the output directory;
/// the name of the file containing the bindings will be based on the
/// sanitized import path of the Go package, with the extension `.rs`.
///
/// For example, if the Go package's import path is `example.com/foo/bar`,
/// the Rust bindings will be written to `OUT_DIR/example_com_foo_bar.rs`,
/// which will be available at compile time as the environment variable
/// `GEN_EXAMPLE_COM_FOO_BAR`.
///
/// # Errors
///
/// This function will return an error if the Go package cannot be loaded,
/// the Go library cannot be built, or the Rust bindings cannot be generated.
fn go_build(src: &Utf8Path, out: &Utf8Path, mode: BuildMode, vcs: BuildVcs) -> Result<()> {
    eprintln!("building Go package at {src}");

    // Load the package info.
    let pkg = Package::load(src, vcs)?;

    // Build the package as a C archive.
    let (lib_name, lib_header) = pkg.build(out, mode, vcs)?;

    // Determine the output path and variable to expose to the Rust compiler.
    let out_path = out.join(&lib_name).with_extension("rs");
    let var_name = format!("GEN_{}", lib_name.to_uppercase());
    let callback = Box::new(bindgen::CargoCallbacks::new());

    // Generate the Rust bindings at the output path.
    bindgen::Builder::default()
        .header(lib_header)
        .parse_callbacks(callback)
        .generate()?
        .write_to_file(&out_path)?;

    // Expose the output path to the Rust compiler.
    println!("cargo:rustc-env={var_name}={out_path}");

    Ok(())
}

/// Holds metadata about a Go package, as returned by `go list -json .`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Package {
    /// Full path to the package directory.
    dir: Utf8PathBuf,

    /// The go import path.
    import_path: String,

    /// Go files in the package.
    #[serde(default)]
    go_files: Vec<String>,

    /// Cgo files in the package.
    #[serde(default)]
    cgo_files: Vec<String>,
}

impl Package {
    fn load(package: &Utf8Path, vcs: BuildVcs) -> Result<Self> {
        eprintln!("loading Go package at {package}");

        let mut cmd = go!("list", "-json", "-buildvcs={vcs}", ".");

        match cmd.current_dir(package).output()? {
            out if out.status.success() => {
                eprintln!("loaded Go package");
                Ok(serde_json::from_slice(&out.stdout)?)
            }

            out => {
                io::stdout().write_all(&out.stdout)?;
                io::stderr().write_all(&out.stderr)?;
                bail!("failed to load package");
            }
        }
    }

    fn files(&self) -> impl IntoIterator<Item = Utf8PathBuf> {
        let mut files = Vec::new();

        for file in &self.go_files {
            files.push(self.dir.join(file));
        }

        for file in &self.cgo_files {
            files.push(self.dir.join(file));
        }

        files
    }

    fn build(
        &self,
        out: &Utf8Path,
        mode: BuildMode,
        vcs: BuildVcs,
    ) -> Result<(String, Utf8PathBuf)> {
        // Get the name of the package from its sanitized import path.
        let name = (self.import_path).replace(|c: char| !c.is_alphanumeric(), "_");

        // Get the extension for the library.
        let ext = match mode {
            BuildMode::Static => "a",
            BuildMode::Dynamic => DLL_EXTENSION,
        };

        // Build the output names.
        let archive = out.join(format!("lib{name}.{ext}"));
        let header = out.join(format!("lib{name}.h"));

        // Get the buildmode flag value.
        let mode_flag = match mode {
            BuildMode::Static => "c-archive",
            BuildMode::Dynamic => "c-shared",
        };

        // Whether to use VCS stamping.
        let vcs_flag = match vcs {
            BuildVcs::True => "true",
            BuildVcs::False => "false",
        };

        // Build the Go library.
        let mut cmd = go!("build", "-buildmode={mode_flag}", "-buildvcs={vcs_flag}",);

        cmd.arg("-o").arg(archive);
        cmd.args(&self.go_files);
        cmd.args(&self.cgo_files);
        cmd.current_dir(&self.dir);

        match cmd.output()? {
            out if out.status.success() => {
                eprintln!("built Go library");
            }

            out => {
                io::stdout().write_all(&out.stdout)?;
                io::stderr().write_all(&out.stderr)?;
                bail!("failed to build Go library");
            }
        }

        // Get the link lib kind.
        let link_kind = match mode {
            BuildMode::Static => "static",
            BuildMode::Dynamic => "dylib",
        };

        // Tell cargo to link the Go lib.
        println!("cargo:rustc-link-search={out}");
        println!("cargo:rustc-link-lib={link_kind}={name}");

        // Tell cargo to rerun if the Go files change.
        for file in self.files() {
            println!("cargo:rerun-if-changed={file}");
        }

        Ok((name, header))
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! cmd {
    ($cmd:expr $(, $arg:expr)* $(,)?) => {{
        let mut cmd = ::std::process::Command::new($cmd);

        $(
            cmd.arg(format!($arg));
        )*

        cmd
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! go {
    ($($arg:expr),* $(,)?) => {
        $crate::cmd!("go" $(, $arg)*)
    };
}
