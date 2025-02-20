//! Collection of interfaces required for OS integration.
mod keychain;

use std::{
    fs::{self},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub use keychain::*;
use tokio::task::spawn_blocking;

/// This is a replacement for `fs::write` that writes to a tempfile and then renames it to `path`.
pub fn safe_write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let mut f = tempfile::NamedTempFile::new()?;
    f.write_all(contents.as_ref())?;
    let temp_path = f.into_temp_path();
    fs::rename(temp_path, path)?;
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
        Err(_) => Err(io::Error::new(
            io::ErrorKind::Other,
            "background task panicked",
        )),
    }
}
