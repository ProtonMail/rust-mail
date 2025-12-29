use itertools::Itertools;
use stash::stash::{StashError, Tether};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::{fs, task, time};
use tracing::{Instrument, debug, error, info, instrument, warn};
use walkdir::WalkDir;

const DB_FILE_EXTS: &[&str] = &["db", "db-wal", "db-shm"];

#[instrument(skip_all)]
pub async fn drop_database_tables(mut tether: Tether) -> Result<(), StashError> {
    info!("Dropping database tables");

    tether.execute("PRAGMA foreign_keys = OFF", vec![]).await?;

    let tables = tether
        .query_values::<_, String>("SELECT name FROM sqlite_master WHERE type='table'", vec![])
        .await?;

    let result = tether
        .tx(async |tx| -> Result<(), StashError> {
            for table in tables {
                debug!(?table, "Dropping table");

                let query = format!("DROP TABLE IF EXISTS {table};");

                if let Err(err) = tx.execute(query, vec![]).await {
                    warn!("Couldn't drop table `{table}`: {err}");
                }
            }

            Ok(())
        })
        .await;

    tether.execute("PRAGMA foreign_keys = ON", vec![]).await?;

    result
}

#[instrument]
pub async fn rename_database_files(path: &Path) {
    info!("Renaming database files");

    let Ok(mut path) = fs::read_dir(path).await else {
        warn!("Couldn't open directory, aborting rename");
        return;
    };

    let mut to_rename = vec![];

    while let Ok(Some(entry)) = path.next_entry().await {
        let path = entry.path();

        let Some(ext) = path.extension().and_then(OsStr::to_str) else {
            continue;
        };

        if DB_FILE_EXTS.contains(&ext) {
            to_rename.push((ext.to_string(), path));
        }
    }

    for (ext, path) in to_rename {
        let new_path = path.with_extension(format!("{ext}.nuked"));

        if let Err(err) = fs::rename(&path, new_path).await {
            error!("Couldn't rename file `{}`: {err}", path.display());
        }
    }
}

#[instrument]
pub async fn remove_dir(path: &Path) {
    info!("Removing directory");

    let result = fs::remove_dir_all(path).await;

    // `remove_dir_all()` tends to fail for larger directories on Windows[1] -
    // in cases like those fall back to manual drop.
    //
    // https://github.com/rust-lang/rust/issues/29497
    if result.is_err() {
        debug!("remove_dir_all() failed, performing a manual cleanup");

        remove_dir_manual(path).await;

        let _ = fs::remove_dir_all(path).await;
    }
}

#[instrument]
async fn remove_dir_manual(path: &Path) {
    let path = path.to_owned();

    let Ok(paths) = task::spawn_blocking(move || {
        WalkDir::new(path)
            .max_depth(4)
            .into_iter()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let meta = entry.metadata().ok()?;

                if meta.is_file() {
                    Some(entry.into_path())
                } else {
                    None
                }
            })
            .collect_vec()
    })
    .await
    else {
        error!("Couldn't find files to remove, the closure panicked");
        return;
    };

    let paths = remove_files(&paths).await;

    if !paths.is_empty() {
        remove_files_in_background(&paths);
    }
}

#[instrument(skip_all)]
async fn remove_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut remaining = vec![];

    for file in paths {
        if file.exists()
            && let Err(err) = fs::remove_file(file).await
        {
            warn!("Couldn't remove `{}`: {err}", file.display());

            let ext = file
                .extension()
                .and_then(|ext| ext.to_str())
                .map_or("nuked".to_string(), |ext| format!("{ext}.nuked"));

            let new_path = file.with_extension(ext);

            match fs::rename(file, &new_path).await {
                Ok(()) => remaining.push(new_path),
                Err(_) => remaining.push(file.clone()),
            }
        }
    }

    remaining
}

#[instrument(skip_all)]
fn remove_files_in_background(paths: &[PathBuf]) {
    let mut paths = paths.to_vec();

    task::spawn(
        async move {
            let started_at = Instant::now();
            let timeout = Duration::from_secs(5);
            let retry_every = Duration::from_millis(100);

            loop {
                time::sleep(retry_every).await;

                paths = remove_files(&paths).await;

                if paths.is_empty() {
                    debug!("Whole path was cleared in the background");
                    break;
                }

                if started_at.elapsed() >= timeout {
                    error!("Task timed out");
                    break;
                }
            }
        }
        .in_current_span(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::TestContext;
    use stash::params;
    use std::path::Path;
    use tempfile::TempDir;

    #[tokio::test]
    async fn remove_dir_given_non_existing_path() {
        let path = Path::new("./this/path/does/not_exist");

        assert!(!path.exists());

        remove_dir(path).await;
    }

    #[tokio::test]
    async fn remove_dir_given_empty_directory() {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        assert!(path.exists());

        remove_dir(path).await;

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn remove_dir_given_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let canary_body = "First line.\nSecond line.\nThird line.\n";
        let canary_path = path.join("file.txt");

        fs::write(&canary_path, canary_body.as_bytes())
            .await
            .unwrap();

        assert!(canary_path.exists());

        remove_dir(&canary_path).await;

        assert!(!canary_path.exists());
    }

    #[tokio::test]
    async fn remove_dir_smoke() {
        let dir = TempDir::new().unwrap();
        let original_path = dir.path();
        let nested_path = original_path.join("one/two/three");

        fs::create_dir_all(&nested_path).await.unwrap();

        let canary_body = "First line.\nSecond line.\nThird line.\n";
        let canary_path = nested_path.join("four.txt");

        fs::write(&canary_path, canary_body.as_bytes())
            .await
            .unwrap();

        assert!(canary_path.exists());

        remove_dir_manual(original_path).await;

        assert!(!canary_path.exists());
        assert!(original_path.exists());
    }

    #[tokio::test]
    async fn remove_dir_manual_smoke() {
        let dir = TempDir::new().unwrap();
        let original_path = dir.path();
        let nested_path = original_path.join("one/two/three/four/five");

        fs::create_dir_all(&nested_path).await.unwrap();

        let canary_body = "First line.\nSecond line.\nThird line.\n";
        let canary_path = nested_path.join("six.txt");

        fs::write(&canary_path, canary_body.as_bytes())
            .await
            .unwrap();

        assert!(canary_path.exists());

        remove_dir_manual(original_path).await;

        assert!(canary_path.exists());
        assert!(original_path.exists());
    }

    #[tokio::test]
    async fn drop_database_tables_smoke() {
        let ctx = TestContext::new().await;
        let uctx = ctx.user_context().await;
        let tether = uctx.stash().connection().await.unwrap();

        tether
            .execute("CREATE TABLE foos (id INT NOT NULL PRIMARY KEY)", vec![])
            .await
            .unwrap();

        tether
            .execute("CREATE TABLE bars (id INT NOT NULL PRIMARY KEY)", vec![])
            .await
            .unwrap();

        assert!(has_table(&tether, "foos").await);
        assert!(has_table(&tether, "bars").await);

        drop_database_tables(tether).await.unwrap();

        let tether = uctx.stash().connection().await.unwrap();

        assert!(!has_table(&tether, "foos").await);
        assert!(!has_table(&tether, "bars").await);
    }

    async fn has_table(tether: &Tether, name: &str) -> bool {
        tether
            .query_value_opt::<u32>(
                "SELECT 1 AS value FROM sqlite_master WHERE name = ?",
                params![name.to_owned()],
            )
            .await
            .unwrap()
            .is_some()
    }
}
