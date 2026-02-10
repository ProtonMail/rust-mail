//! Collection of interfaces required for OS integration.
mod keychain;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use keychain::*;
use tokio::task::spawn_blocking;

/// This is a replacement for `fs::write` that writes to a tempfile and then renames it to `path`.
pub fn safe_write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();
    let mut filename = path.file_name().unwrap_or_default().to_owned();
    filename.push(".tmp");
    let tmp_file = path.with_file_name(filename);
    fs::write(&tmp_file, contents)?;
    fs::rename(&tmp_file, path)?;
    Ok(())
}

/// This is a replacement for `fs::write` that writes to a tempfile and then renames it to `path`.
pub async fn safe_write_async<C: AsRef<[u8]> + Send + 'static>(
    path: impl Into<PathBuf>,
    contents: C,
) -> io::Result<()> {
    let path: PathBuf = path.into();
    match spawn_blocking(move || safe_write(path, contents)).await {
        Ok(res) => res,
        Err(_) => Err(io::Error::other("background task panicked")),
    }
}

/// Replaces all instances of the platform separator with `_`.
///
/// Note: this is only intended for the file name component, not a full path.
#[must_use]
pub fn sanitize_filename(filename: &str) -> String {
    filename.replace(std::path::MAIN_SEPARATOR, "_")
}
