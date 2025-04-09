use std::{ffi::OsStr, path::Path, sync::Arc};

use futures::future::try_join_all;
use stash::stash::{StashError, Tether};
use tokio::fs;

use crate::{Context, pin_code::PinError};

pub const DB_EXTENSIONS: &[&str] = &["db", "db-wal", "db-shm"];
const QUERY_LIST_TABLES: &str = "SELECT name as value FROM sqlite_master WHERE type='table'";

pub async fn nuke_core_application_data(ctx: Arc<Context>) -> Result<(), PinError> {
    tracing::warn!("Fetch all logged in users.");
    let all_user_ctxs = ctx.get_all_logged_in_user_ctx().await?;
    let users = ctx.get_accounts().await?;

    tracing::warn!("Logout and delete all accounts");
    for user in users {
        ctx.logout_account(user.remote_id.clone()).await?;
    }

    tracing::warn!("Remove all user data and kill all background tasks");
    let iter = all_user_ctxs.iter().map(|ctx| async {
        let tether = ctx.stash().connection();

        ctx.cancel_all_tasks();
        drop_all_tables_in_database(tether).await?;

        Result::<(), PinError>::Ok(())
    });

    try_join_all(iter).await?;

    tracing::warn!("Remove all remaining account data");
    let tether = ctx.account_stash().connection();

    drop_all_tables_in_database(tether).await?;

    tracing::warn!("Remove all cached filesystem data");
    if let Err(e) = fs::remove_dir_all(ctx.get_cache_location()).await {
        tracing::error!("Could not remove cached data in filesystem, details: `{e}`");
    }

    tracing::warn!("Archive user databases");
    rename_database_files(ctx.get_user_db_location()).await;

    tracing::warn!("Archive account database");
    rename_database_files(ctx.get_account_db_location()).await;

    tracing::warn!("Application's data has been cleared successfuly");

    Ok(())
}

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
