use crate::datatypes::DeletedItemType;
use crate::models::{Conversation, Message};
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_sqlite3::rusqlite::Transaction;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use std::collections::HashSet;

#[cfg(test)]
#[path = "../tests/models/deleted_item.rs"]
mod tests;

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("deleted_items")]
pub struct DeletedItem {
    #[IdField]
    pub remote_id: String,

    #[DbField]
    pub item_type: DeletedItemType,

    #[DbField]
    pub deleted_at: UnixTimestamp,
}

impl DeletedItem {
    pub fn new(remote_id: String, item_type: DeletedItemType) -> Self {
        Self {
            remote_id,
            item_type,
            deleted_at: UnixTimestamp::now(),
        }
    }

    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // We do not update to not extend the lifetime of tombstone
        bond.execute(
            format!(
                "INSERT OR IGNORE INTO {} (remote_id, item_type, deleted_at) VALUES (?, ?, ?)",
                Self::table_name()
            ),
            params![self.remote_id.clone(), self.item_type, self.deleted_at],
        )
        .await?;

        Ok(())
    }

    pub async fn find_deleted_by_remote_ids(
        remote_ids: impl IntoIterator<Item = impl AsRef<str>>,
        item_type: DeletedItemType,
        tether: &Tether,
    ) -> Result<HashSet<String>, StashError> {
        use stash::exports::ToSql;
        use stash::utils::placeholders;

        let mut params: Vec<Box<dyn ToSql + Send>> = vec![Box::new(item_type)];
        for id in remote_ids {
            params.push(Box::new(id.as_ref().to_string()));
        }

        // Early exit if no IDs provided (only item_type in params)
        if params.len() <= 1 {
            return Ok(HashSet::new());
        }

        let in_placeholders = placeholders(&params[1..]);
        let query = format!(
            "SELECT remote_id FROM {} WHERE item_type = ? AND remote_id IN ({})",
            Self::table_name(),
            in_placeholders
        );

        let deleted_ids = tether.query_values::<_, String>(query, params).await?;

        Ok(deleted_ids.into_iter().collect())
    }

    pub fn remove_tombstone_sync(
        remote_id: &str,
        item_type: DeletedItemType,
        tx: &Transaction<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            &format!(
                "DELETE FROM {} WHERE remote_id = ? AND item_type = ?",
                Self::table_name()
            ),
            (remote_id, item_type),
        )?;
        Ok(())
    }

    /// Verify and cleanup deleted items.
    ///
    /// This method runs after each event poll to:
    /// 1. Remove deleted items that have been re-added to their original tables
    /// 2. Remove deleted items that are older than 1 day (stale tombstones)
    ///
    pub async fn verify_and_cleanup(bond: &Bond<'_>) -> Result<(), StashError> {
        const RETENTION_SECONDS: u64 = 86400; // 1 day

        let all_deleted_items = Self::all(bond).await?;

        if all_deleted_items.is_empty() {
            return Ok(());
        }

        let cutoff = UnixTimestamp::now().saturating_sub(RETENTION_SECONDS);

        for item in all_deleted_items {
            if item.deleted_at < cutoff {
                bond.execute(
                    format!(
                        "DELETE FROM {} WHERE remote_id = ? AND item_type = ?",
                        Self::table_name()
                    ),
                    params![item.remote_id.clone(), item.item_type],
                )
                .await?;
            }

            match item.item_type {
                DeletedItemType::Message => {
                    Message::delete_by_remote_id(item.remote_id.clone().into(), bond).await?
                }
                DeletedItemType::Conversation => {
                    Conversation::delete_by_remote_id(item.remote_id.clone().into(), bond).await?
                }
                DeletedItemType::Label => {
                    Label::delete_by_remote_id(item.remote_id.clone().into(), bond).await?
                }
            };
        }

        Ok(())
    }
}
