use stash::stash::{Bond, StashError};

pub async fn create_paginator_tables(tx: &Bond<'_>) -> Result<(), StashError> {
    tx.execute(
        r#"
            CREATE TABLE mail_conversation_scroll_data (
                local_label_id INTEGER NOT NULL,
                unread INTEGER NOT NULL DEFAULT 0,
                remote_conversation_id TEXT NOT NULL,
                conversation_time INTEGER NOT NULL,
                display_order INTEGER NOT NULL,
                PRIMARY KEY (local_label_id, unread),

                CONSTRAINT local_label_id_mail_conversation_scroll_data
                    FOREIGN KEY (local_label_id)
                    REFERENCES labels (local_id)
                    ON DELETE CASCADE
            )
        "#,
        vec![],
    )
    .await?;
    tx.execute(
        r#"
            CREATE TABLE mail_message_scroll_data (
                local_label_id INTEGER NOT NULL,
                unread INTEGER NOT NULL DEFAULT 0,
                remote_message_id TEXT NOT NULL,
                message_time INTEGER NOT NULL,
                display_order INTEGER NOT NULL,
                PRIMARY KEY (local_label_id, unread),

                CONSTRAINT local_label_id_mail_message_scroll_data
                    FOREIGN KEY (local_label_id)
                    REFERENCES labels (local_id)
                    ON DELETE CASCADE
            )
        "#,
        vec![],
    )
    .await?;
    tx.execute(
        r#"
        CREATE TABLE mail_search_scroll_data (
            local_message_id INTEGER PRIMARY KEY,
            display_order INTEGER NOT NULL,
            time INTEGER NOT NULL,

            CONSTRAINT local_message_id_mail_search_scroll_data
                FOREIGN KEY (local_message_id)
                REFERENCES messages (local_id)
                ON DELETE CASCADE
        )
    "#,
        vec![],
    )
    .await?;

    Ok(())
}
