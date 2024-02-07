#![allow(unused)]
use crate::common::{FolderId, LabelId, Message, MessageId};
use proton_api_core::exports::anyhow;
use proton_sqlite3::rusqlite;
use rusqlite::{params_from_iter, OptionalExtension};
use std::str::FromStr;

pub struct TestLocalSource {
    conn: proton_sqlite3::SqliteConnection,
}

impl TestLocalSource {
    pub fn new(
        connection_pool: &proton_sqlite3::SqliteConnectionPool,
    ) -> Result<Self, rusqlite::Error> {
        let conn = connection_pool.acquire()?;
        Ok(Self { conn })
    }

    pub fn new_with_init(
        connection_pool: &proton_sqlite3::SqliteConnectionPool,
    ) -> Result<Self, rusqlite::Error> {
        let mut conn = connection_pool.acquire()?;
        conn.tx(|tx| -> rusqlite::Result<()> {
            let mut source = TestLocalSourceTransaction::new(tx);

            source.create_tables()?;

            Ok(())
        })?;
        Ok(Self { conn })
    }

    pub fn tx<
        R,
        E: From<rusqlite::Error>,
        F: FnOnce(TestLocalSourceTransaction) -> Result<R, E>,
    >(
        &mut self,
        f: F,
    ) -> Result<R, E> {
        let mut tx = self.conn.transaction()?;
        let r = {
            let ttx = TestLocalSourceTransaction::new(&mut tx);
            let r = (f)(ttx)?;
            r
        };
        tx.commit()?;
        Ok(r)
    }
}

#[mockall::automock]
pub trait RemoteSource {
    fn get_messages(&self) -> Result<Vec<Message>, proton_api_core::http::HttpRequestError>;

    fn get_message(
        &self,
        id: MessageId,
    ) -> Result<Message, proton_api_core::http::HttpRequestError>;

    fn move_messages(
        &self,
        folder_id: FolderId,
        message_ids: &[MessageId],
    ) -> Result<(), proton_api_core::http::HttpRequestError>;

    fn mark_messages_read(
        &self,
        value: bool,
        message_ids: &[MessageId],
    ) -> Result<(), proton_api_core::http::HttpRequestError>;

    fn delete_messages(
        &self,
        id: &[MessageId],
    ) -> Result<(), proton_api_core::http::HttpRequestError>;
}

pub struct TestLocalSourceTransaction<'r, 'c> {
    tx: &'r mut rusqlite::Transaction<'c>,
}

impl<'r, 'c> TestLocalSourceTransaction<'r, 'c> {
    pub fn new(tx: &'r mut rusqlite::Transaction<'c>) -> Self {
        Self { tx }
    }

    fn create_tables(&mut self) -> rusqlite::Result<()> {
        // Folder table
        self.tx.execute("CREATE TABLE folders (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE)",())?;
        self.tx.execute("CREATE TABLE labels (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE)",())?;
        // Message tables
        self.tx.execute(
            "CREATE TABLE messages (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, read INTEGER, deleted INTEGER DEFAULT 0)",
            (),
        )?;
        // Message <-> folder
        self.tx.execute("CREATE TABLE message_folders (message INTEGER NOT NULL UNIQUE, folder INTEGER NOT NULL, remote INTEGER NOT NULL,
        PRIMARY KEY (message, folder),
        CONSTRAINT `message_ref_delete` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON DELETE CASCADE,
        CONSTRAINT `message_ref_update` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON UPDATE CASCADE,
        CONSTRAINT `folder_ref_delete` FOREIGN KEY (`folder`) REFERENCES `folders` (`id`) ON DELETE CASCADE,
        CONSTRAINT `folder_ref_update` FOREIGN KEY (`folder`) REFERENCES `folders` (`id`) ON UPDATE CASCADE,
        CONSTRAINT `remote_ref` FOREIGN KEY (`remote`) REFERENCES `folders` (`id`) ON DELETE CASCADE
        )",())?;

        // Message <-> labels
        self.tx.execute("CREATE TABLE message_labels (message INTEGER NOT NULL, label INTEGER NOT NULL,
        PRIMARY KEY (message, label),
        CONSTRAINT `message_ref_delete` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON DELETE CASCADE,
        CONSTRAINT `message_ref_update` FOREIGN KEY (`message`) REFERENCES `messages` (`id`) ON UPDATE CASCADE,
        CONSTRAINT `label_ref_delete` FOREIGN KEY (`label`) REFERENCES `labels` (`id`) ON DELETE CASCADE,
        CONSTRAINT `label_ref_update` FOREIGN KEY (`label`) REFERENCES `labels` (`id`) ON UPDATE CASCADE
        )",())?;

        Ok(())
    }
    pub fn create_message(&mut self, read: bool) -> Result<MessageId, anyhow::Error> {
        let id = self.tx.query_row(
            "INSERT into messages (read) VALUES (?) RETURNING id",
            [read],
            |r| r.get(0),
        )?;

        Ok(id)
    }

    pub fn get_message(&self, id: MessageId) -> Result<Option<Message>, anyhow::Error> {
        let m = self.tx.query_row("SELECT messages.id, messages.read, message_folders.folder, GROUP_CONCAT(message_labels.label) as labels FROM messages
LEFT JOIN message_folders ON messages.id=message_folders.message
LEFT JOIN message_labels ON messages.id=message_labels.message
WHERE messages.id = ? GROUP BY messages.id LIMIT 1
", [id], |v| {
            let labels = v.get::<usize,Option<String>>(3)?;
            let labels = if let Some(labels) = labels { labels.split(',').map(|v| LabelId(u32::from_str(v).expect("failed to parse integer"))).collect::<Vec<_>>() } else { Vec::new()};
            Ok(Message {
                id: v.get(0)?,
                folder:v.get(2)?,
                labels,
                read: v.get(1)?
            })
        }).optional()?;
        Ok(m)
    }

    pub fn get_messages(&self, ids: &[MessageId]) -> Result<Vec<Message>, anyhow::Error> {
        let mut stmt = self.tx.prepare(&format!(
            "SELECT messages.id, messages.read, message_folders.folder, GROUP_CONCAT(message_labels.label) as labels FROM messages
LEFT JOIN message_folders ON messages.id=message_folders.message
LEFT JOIN message_labels ON messages.id=message_labels.message
WHERE messages.id IN ({}) AND messages.deleted=FALSE GROUP BY messages.id ", gen_variable_args("?", ids.len())))?;
        let mut messages = Vec::with_capacity(ids.len());

        let sql_messages = stmt.query_map(params_from_iter(ids.iter()), |v| {
            let labels = v.get::<usize, Option<String>>(3)?;
            let labels = if let Some(labels) = labels {
                labels
                    .split(',')
                    .map(|v| LabelId(u32::from_str(v).expect("failed to parse integer")))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            Ok(Message {
                id: v.get(0)?,
                folder: v.get(2)?,
                labels,
                read: v.get(1)?,
            })
        })?;

        for m in sql_messages {
            messages.push(m?);
        }
        Ok(messages)
    }

    pub fn get_messages_with_deleted(
        &self,
        ids: &[MessageId],
    ) -> Result<Vec<Message>, anyhow::Error> {
        let mut stmt = self.tx.prepare(&format!(
            "SELECT messages.id, messages.read, message_folders.folder, GROUP_CONCAT(message_labels.label) as labels FROM messages
LEFT JOIN message_folders ON messages.id=message_folders.message
LEFT JOIN message_labels ON messages.id=message_labels.message
WHERE messages.id IN ({})  GROUP BY messages.id ", gen_variable_args("?", ids.len())))?;
        let mut messages = Vec::with_capacity(ids.len());

        let sql_messages = stmt.query_map(params_from_iter(ids.iter()), |v| {
            let labels = v.get::<usize, Option<String>>(3)?;
            let labels = if let Some(labels) = labels {
                labels
                    .split(',')
                    .map(|v| LabelId(u32::from_str(v).expect("failed to parse integer")))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            Ok(Message {
                id: v.get(0)?,
                folder: v.get(2)?,
                labels,
                read: v.get(1)?,
            })
        })?;

        for m in sql_messages {
            messages.push(m?);
        }
        Ok(messages)
    }

    pub fn add_message_to_label(
        &mut self,
        message_ids: &[MessageId],
        label_id: LabelId,
    ) -> Result<(), anyhow::Error> {
        let mut stmt = self
            .tx
            .prepare("INSERT OR IGNORE into message_labels (message,label) VALUES (?,?)")?;

        for id in message_ids {
            stmt.execute((id, label_id))?;
        }
        Ok(())
    }

    pub fn remove_message_from_label(
        &mut self,
        message_ids: &[MessageId],
        label_id: LabelId,
    ) -> Result<(), anyhow::Error> {
        let mut stmt = self
            .tx
            .prepare("DELETE FROM message_labels WHERE message=? AND label=?")?;

        for id in message_ids {
            stmt.execute((id, label_id))?;
        }
        Ok(())
    }

    pub fn move_message_to_folder(
        &mut self,
        message_ids: &[MessageId],
        to_folder_id: FolderId,
    ) -> Result<(), anyhow::Error> {
        let mut stmt = self
            .tx
            .prepare("INSERT INTO message_folders(message,folder, remote) VALUES (?,?,?) ON CONFLICT (message) DO UPDATE SET folder=excluded.folder")?;

        for id in message_ids {
            stmt.execute((id, to_folder_id, to_folder_id))?;
        }
        Ok(())
    }

    pub fn mark_messages_read(
        &mut self,
        value: bool,
        ids: &[MessageId],
    ) -> Result<(), anyhow::Error> {
        let mut stmt = self.tx.prepare("UPDATE messages SET read=? WHERE id=?")?;
        for id in ids {
            stmt.execute((value, id))?;
        }
        Ok(())
    }

    pub fn mark_messages_deleted(
        &mut self,
        value: bool,
        ids: &[MessageId],
    ) -> Result<(), anyhow::Error> {
        let mut stmt = self
            .tx
            .prepare("UPDATE messages SET deleted=? WHERE id=?")?;
        for id in ids {
            stmt.execute((value, id))?;
        }
        Ok(())
    }

    pub fn delete_message(&mut self, message_ids: &[MessageId]) -> Result<(), anyhow::Error> {
        let mut stmt = self.tx.prepare("DELETE FROM messages WHERE id=?")?;
        for id in message_ids {
            stmt.execute([id])?;
        }
        Ok(())
    }
    pub fn create_folder(&mut self, name: &str) -> Result<FolderId, anyhow::Error> {
        let folder_id = self.tx.query_row(
            "INSERT INTO folders (name) VALUES (?) RETURNING id",
            [name],
            |v| v.get(0),
        )?;
        Ok(folder_id)
    }

    pub fn rename_folder(&mut self, id: FolderId, name: &str) -> Result<(), anyhow::Error> {
        self.tx
            .execute("UPDATE folders SET name=? WHERE id=?", (name, id))?;
        Ok(())
    }

    pub fn delete_folder(&mut self, id: FolderId) -> Result<(), anyhow::Error> {
        self.tx.execute("DELETE FROM folders WHERE id=?", [id])?;
        Ok(())
    }

    pub fn create_label(&mut self, name: &str) -> Result<LabelId, anyhow::Error> {
        let label_id = self.tx.query_row(
            "INSERT INTO labels (name) VALUES (?) RETURNING id",
            [name],
            |v| v.get(0),
        )?;
        Ok(label_id)
    }

    pub fn rename_label(&mut self, id: LabelId, name: &str) -> Result<(), anyhow::Error> {
        self.tx
            .execute("UPDATE label SET name=? WHERE id=?", (name, id))?;
        Ok(())
    }

    pub fn delete_label(&mut self, id: LabelId) -> Result<(), anyhow::Error> {
        self.tx.execute("DELETE FROM labels WHERE id=?", [id])?;
        Ok(())
    }

    pub fn get_folder_name(&self, id: FolderId) -> Result<Option<String>, anyhow::Error> {
        let name = self
            .tx
            .query_row("SELECT name FROM folders WHERE id = ? LIMIT 1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(name)
    }

    // Dep tracking

    pub fn update_move_message_dependency(
        &mut self,
        to: FolderId,
        ids: &[MessageId],
    ) -> Result<(), anyhow::Error> {
        let mut stmt = self
            .tx
            .prepare("UPDATE message_folders SET remote =? WHERE message = ?")?;
        for id in ids {
            stmt.execute((to, id))?;
        }

        Ok(())
    }

    pub fn get_move_message_state(
        &self,
        ids: &[MessageId],
    ) -> Result<Vec<(MessageId, FolderId)>, anyhow::Error> {
        let mut stmt = self.tx.prepare(&format!(
            "SELECT message, remote FROM message_folders WHERE message IN ({})",
            gen_variable_args("?", ids.len())
        ))?;

        let mut result = Vec::with_capacity(ids.len());
        let iter = stmt.query_map(params_from_iter(ids), |row| Ok((row.get(0)?, row.get(1)?)))?;

        for item in iter {
            result.push(item?);
        }

        Ok(result)
    }
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
