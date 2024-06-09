#![allow(unused)]
use crate::common::{FolderId, LabelId, Message, MessageId};
use proton_api_core::exports::anyhow;
use proton_sqlite3::{rusqlite, SqliteTransaction};
use rusqlite::{params_from_iter, OptionalExtension};
use serde::{Deserialize, Serialize};
use stash::datatypes::{QueryResultString, QueryResultU64};
use stash::exports::ToSql;
use stash::macros::DbRecord;
use stash::params;
use stash::stash::{Stash, StashError, Tether};
use std::str::FromStr;

pub struct TestLocalSource {
    stash: Stash,
}

impl TestLocalSource {
    pub fn new(stash: Stash) -> Result<Self, StashError> {
        Ok(Self { stash })
    }

    pub async fn new_with_init(stash: Stash) -> Result<Self, StashError> {
        let tx = stash.transaction().await?;
        let mut source = TestLocalSourceTransaction::new(tx.clone());
        source.create_tables().await?;
        tx.commit().await?;
        Ok(Self { stash })
    }

    pub async fn tx<R, E: From<StashError>, F: Fn(TestLocalSourceTransaction) -> Result<R, E>>(
        &mut self,
        f: F,
    ) -> Result<R, E> {
        let tx = self.stash.transaction().await?;
        let ttx = TestLocalSourceTransaction::new(tx.clone());
        let r = (f)(ttx)?;
        tx.commit().await?;
        Ok(r)
    }
}

#[mockall::automock]
pub trait RemoteSource: Send + Sync {
    fn get_messages(&self) -> Result<Vec<Message>, proton_api_core::http::RequestError>;

    fn get_message(&self, id: MessageId) -> Result<Message, proton_api_core::http::RequestError>;

    fn move_messages(
        &self,
        folder_id: FolderId,
        message_ids: &[MessageId],
    ) -> Result<(), proton_api_core::http::RequestError>;

    fn mark_messages_read(
        &self,
        value: bool,
        message_ids: &[MessageId],
    ) -> Result<(), proton_api_core::http::RequestError>;

    fn delete_messages(&self, id: &[MessageId]) -> Result<(), proton_api_core::http::RequestError>;
}

pub struct TestLocalSourceTransaction {
    tx: Tether,
}

impl TestLocalSourceTransaction {
    pub fn new(tx: Tether) -> Self {
        Self { tx }
    }

    async fn create_tables(&mut self) -> Result<(), StashError> {
        // Folder table
        self.tx.execute("CREATE TABLE folders (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE)",vec![]).await?;
        self.tx.execute("CREATE TABLE labels (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE)",vec![]).await?;
        // Message tables
        self.tx.execute(
            "CREATE TABLE messages (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, read INTEGER, deleted INTEGER DEFAULT 0)",
            vec![],
        ).await?;
        // Message <-> folder
        self.tx.execute("CREATE TABLE message_folders (message INTEGER NOT NULL UNIQUE, folder INTEGER NOT NULL, remote INTEGER NOT NULL,
        PRIMARY KEY (message, folder),
        CONSTRAINT `message_ref_delete` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON DELETE CASCADE,
        CONSTRAINT `message_ref_update` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON UPDATE CASCADE,
        CONSTRAINT `folder_ref_delete` FOREIGN KEY (`folder`) REFERENCES `folders` (`id`) ON DELETE CASCADE,
        CONSTRAINT `folder_ref_update` FOREIGN KEY (`folder`) REFERENCES `folders` (`id`) ON UPDATE CASCADE,
        CONSTRAINT `remote_ref` FOREIGN KEY (`remote`) REFERENCES `folders` (`id`) ON DELETE CASCADE
        )",vec![]).await?;

        // Message <-> labels
        self.tx.execute("CREATE TABLE message_labels (message INTEGER NOT NULL, label INTEGER NOT NULL,
        PRIMARY KEY (message, label),
        CONSTRAINT `message_ref_delete` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON DELETE CASCADE,
        CONSTRAINT `message_ref_update` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON UPDATE CASCADE,
        CONSTRAINT `label_ref_delete` FOREIGN KEY (`label`) REFERENCES `labels` (`id`) ON DELETE CASCADE,
        CONSTRAINT `label_ref_update` FOREIGN KEY (`label`) REFERENCES `labels` (`id`) ON UPDATE CASCADE
        )",vec![]).await?;

        Ok(())
    }
    pub async fn create_message(&mut self, read: bool) -> Result<MessageId, anyhow::Error> {
        Ok(MessageId(
            self.tx
                .query::<_, QueryResultU64>(
                    "INSERT into messages (read) VALUES (?) RETURNING id AS value",
                    params![read],
                )
                .await?
                .first()
                .unwrap()
                .value as u32,
        ))
    }

    pub async fn get_message(&self, id: MessageId) -> Result<Option<Message>, anyhow::Error> {
        Ok(self.tx.query::<_, Message>("SELECT messages.rowid AS rowid, messages.id, messages.read, message_folders.folder, GROUP_CONCAT(message_labels.label) as labels FROM messages
LEFT JOIN message_folders ON messages.id=message_folders.message
LEFT JOIN message_labels ON messages.id=message_labels.message
WHERE messages.id = ? GROUP BY messages.id LIMIT 1
", params![id]).await?.into_iter().next())
    }

    pub async fn get_messages(&self, ids: &[MessageId]) -> Result<Vec<Message>, anyhow::Error> {
        #[allow(trivial_casts)]
        Ok(self.tx.query::<_, Message>(&format!(
            "SELECT messages.rowid AS rowid, messages.id, messages.read, message_folders.folder, GROUP_CONCAT(message_labels.label) as labels FROM messages
LEFT JOIN message_folders ON messages.id=message_folders.message
LEFT JOIN message_labels ON messages.id=message_labels.message
WHERE messages.id IN ({}) AND messages.deleted=FALSE GROUP BY messages.id ", gen_variable_args("?", ids.len())), ids.iter().map(|item| Box::new(*item) as Box<dyn ToSql + Send>).collect()).await?)
    }

    pub async fn get_messages_with_deleted(
        &self,
        ids: &[MessageId],
    ) -> Result<Vec<Message>, anyhow::Error> {
        #[allow(trivial_casts)]
        Ok(self.tx.query::<_, Message>(&format!(
            "SELECT messages.rowid AS rowid, messages.id, messages.read, message_folders.folder, GROUP_CONCAT(message_labels.label) as labels FROM messages
LEFT JOIN message_folders ON messages.id=message_folders.message
LEFT JOIN message_labels ON messages.id=message_labels.message
WHERE messages.id IN ({})  GROUP BY messages.id ", gen_variable_args("?", ids.len())), ids.iter().map(|item| Box::new(*item) as Box<dyn ToSql + Send>).collect()).await?)
    }

    pub async fn add_message_to_label(
        &mut self,
        message_ids: &[MessageId],
        label_id: LabelId,
    ) -> Result<(), anyhow::Error> {
        for id in message_ids {
            self.tx
                .execute(
                    "INSERT OR IGNORE into message_labels (message,label) VALUES (?,?)",
                    params![*id, label_id],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn remove_message_from_label(
        &mut self,
        message_ids: &[MessageId],
        label_id: LabelId,
    ) -> Result<(), anyhow::Error> {
        for id in message_ids {
            self.tx
                .execute(
                    "DELETE FROM message_labels WHERE message=? AND label=?",
                    params![*id, label_id],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn move_message_to_folder(
        &mut self,
        message_ids: &[MessageId],
        to_folder_id: FolderId,
    ) -> Result<(), anyhow::Error> {
        for id in message_ids {
            self.tx.execute("INSERT INTO message_folders(message,folder, remote) VALUES (?,?,?) ON CONFLICT (message) DO UPDATE SET folder=excluded.folder", params![*id, to_folder_id, to_folder_id]).await?;
        }
        Ok(())
    }

    pub async fn mark_messages_read(
        &mut self,
        value: bool,
        ids: &[MessageId],
    ) -> Result<(), anyhow::Error> {
        for id in ids {
            self.tx
                .execute("UPDATE messages SET read=? WHERE id=?", params![value, *id])
                .await?;
        }
        Ok(())
    }

    pub async fn mark_messages_deleted(
        &mut self,
        value: bool,
        ids: &[MessageId],
    ) -> Result<(), anyhow::Error> {
        for id in ids {
            self.tx
                .execute(
                    "UPDATE messages SET deleted=? WHERE id=?",
                    params![value, *id],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn delete_message(&mut self, message_ids: &[MessageId]) -> Result<(), anyhow::Error> {
        for id in message_ids {
            self.tx
                .execute("DELETE FROM messages WHERE id=?", params![*id])
                .await?;
        }
        Ok(())
    }
    pub async fn create_folder(&mut self, name: &str) -> Result<FolderId, anyhow::Error> {
        Ok(FolderId(
            self.tx
                .query::<_, QueryResultU64>(
                    "INSERT INTO folders (name) VALUES (?) RETURNING id AS value",
                    params![name.to_owned()],
                )
                .await?
                .first()
                .unwrap()
                .value as u32,
        ))
    }

    pub async fn rename_folder(&mut self, id: FolderId, name: &str) -> Result<(), anyhow::Error> {
        self.tx
            .execute(
                "UPDATE folders SET name=? WHERE id=?",
                params![name.to_owned(), Box::new(id)],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_folder(&mut self, id: FolderId) -> Result<(), anyhow::Error> {
        self.tx
            .execute("DELETE FROM folders WHERE id=?", params![id])
            .await?;
        Ok(())
    }

    pub async fn create_label(&mut self, name: &str) -> Result<LabelId, anyhow::Error> {
        Ok(LabelId(
            self.tx
                .query::<_, QueryResultU64>(
                    "INSERT INTO labels (name) VALUES (?) RETURNING id AS value",
                    params![name.to_owned()],
                )
                .await?
                .first()
                .unwrap()
                .value as u32,
        ))
    }

    pub async fn rename_label(&mut self, id: LabelId, name: &str) -> Result<(), anyhow::Error> {
        self.tx
            .execute(
                "UPDATE label SET name=? WHERE id=?",
                params![name.to_owned(), id],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_label(&mut self, id: LabelId) -> Result<(), anyhow::Error> {
        self.tx
            .execute("DELETE FROM labels WHERE id=?", params![id])
            .await?;
        Ok(())
    }

    pub async fn get_folder_name(&self, id: FolderId) -> Result<Option<String>, anyhow::Error> {
        Ok(self
            .tx
            .query::<_, QueryResultString>(
                "SELECT name AS value FROM folders WHERE id = ? LIMIT 1",
                params![id],
            )
            .await?
            .into_iter()
            .next()
            .map(|item| item.value))
    }

    // Dep tracking

    pub async fn update_move_message_dependency(
        &mut self,
        to: FolderId,
        ids: &[MessageId],
    ) -> Result<(), anyhow::Error> {
        for id in ids {
            self.tx
                .execute(
                    "UPDATE message_folders SET remote =? WHERE message = ?",
                    params![*id, to],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn get_move_message_state(
        &self,
        ids: &[MessageId],
    ) -> Result<Vec<(MessageId, FolderId)>, anyhow::Error> {
        #[allow(trivial_casts)]
        let iter = self
            .tx
            .query::<_, MessageFolder>(
                &format!(
                    "SELECT message, remote FROM message_folders WHERE message IN ({})",
                    gen_variable_args("?", ids.len())
                ),
                ids.iter()
                    .map(|item| Box::new(*item) as Box<dyn ToSql + Send>)
                    .collect(),
            )
            .await?;
        let mut result = Vec::with_capacity(ids.len());
        for item in iter {
            result.push((MessageId(item.message), FolderId(item.remote)));
        }

        Ok(result)
    }
}

#[derive(Clone, DbRecord, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MessageFolder {
    #[DbField]
    pub message: u32,
    #[DbField]
    pub remote: u32,
}

fn gen_variable_args(input: &str, count: usize) -> String {
    debug_assert!(count > 0);
    let mut string = String::with_capacity(input.len() * count + (count - 1));
    string.push_str(input);
    for _ in 1_usize..count {
        string.push(',');
        string.push_str(input);
    }

    return string;
}
