use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use itertools::Itertools;
use stash::stash::{StashError, Tether};
use tokio::{fs, task};
use walkdir::WalkDir;

pub const DB_EXTENSIONS: &[&str] = &["db", "db-wal", "db-shm"];
const QUERY_LIST_TABLES: &str = "SELECT name as value FROM sqlite_master WHERE type='table'";

pub(crate) async fn drop_all_tables_in_database(mut tether: Tether) -> Result<(), StashError> {
    tether.execute("PRAGMA foreign_keys = OFF;", vec![]).await?;

    let table_names = tether
        .query_values::<_, String>(QUERY_LIST_TABLES, vec![])
        .await?;

    let tx_res = tether
        .tx(async |tx| -> Result<(), StashError> {
            for table in table_names {
                let query = format!("DROP TABLE IF EXISTS {table};");
                if let Err(e) = tx.execute(query, vec![]).await {
                    tracing::error!("Could not drop table: `{table}`, details: `{e}`");
                }
            }

            Ok(())
        })
        .await;

    tether.execute("PRAGMA foreign_keys = ON;", vec![]).await?;

    tx_res
}

pub async fn rename_database_files(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let Ok(mut db_dir) = fs::read_dir(path).await else {
        tracing::error!("Could not read database directory, aborting archive");
        return;
    };
    let mut to_rename = vec![];

    while let Ok(Some(entry)) = db_dir.next_entry().await {
        let path = entry.path();
        let Some(extension) = path.extension().and_then(OsStr::to_str) else {
            continue;
        };
        if DB_EXTENSIONS.contains(&extension) {
            to_rename.push((extension.to_string(), path));
        }
    }

    for (extension, path) in to_rename {
        let new_path = path.with_extension(format!("{extension}.nuked"));

        if let Err(e) = fs::rename(path, new_path).await {
            tracing::error!("Could not rename the file, details: `{e}`");
        }
    }
}

pub async fn remove_or_clear_dir_safe(path: impl AsRef<Path>) {
    let path = path.as_ref().to_path_buf();
    // Try remove whole directory.
    // It may unfortunately fail on certain operating systems such as Windows:
    // https://github.com/rust-lang/rust/issues/29497
    let result = fs::remove_dir_all(&path).await;

    // When it fails, fallback to deleting one-by-one
    if result.is_err() {
        let path_clone = path.clone();
        // Get all files paths in max_depth eq 2
        let Ok(all_files) = task::spawn_blocking(move || {
            WalkDir::new(format!("{}/**", path_clone.display()))
                .max_depth(2)
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
            // Unlikely to happen as the closure is non failing
            tracing::error!("Could not join task when gathering all files to remove");
            return;
        };

        let failed = remove_files(&all_files).await;

        if failed.is_empty() {
            // Clean the folder structure
            let _ = fs::remove_dir_all(&path).await;
        } else {
            // We have still some files not removed
            // lets derefer this to the background
            task::spawn(async move {
                let max_wait: Duration = Duration::from_secs(5);
                let retry_interval: Duration = Duration::from_millis(100);
                let start = Instant::now();
                let mut failed = failed;
                loop {
                    tokio::time::sleep(retry_interval).await;
                    failed = remove_files(&failed).await;
                    if failed.is_empty() {
                        tracing::info!("Whole path was cleared in the background");
                        let _ = fs::remove_dir_all(&path).await;
                        break;
                    }
                    if start.elapsed() >= max_wait {
                        tracing::error!("Unfortunatelly we were unable to clear the path.");
                        break;
                    }
                }
            });
        }
    }
}

async fn remove_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut failed = vec![];

    for file in paths {
        if file.exists() {
            if let Err(e) = fs::remove_file(file).await {
                tracing::error!("Could not remove `{}`, details: `{e}`", file.display());
                let ext = file
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map_or("nuked".to_string(), |ext| format!("{ext}.nuked"));
                let new_path = file.with_extension(ext);
                match fs::rename(file, &new_path).await {
                    Ok(()) => failed.push(new_path),
                    Err(_) => failed.push(file.clone()),
                }
            }
        }
    }

    failed
}
