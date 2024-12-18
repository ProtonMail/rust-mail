use stash::stash::{Bond, StashError};

pub async fn create_paginator_tables(tx: &Bond<'_>) -> Result<(), StashError> {
    //TODO: foreign key references for label id - cascasde delete
    tx.execute(
        r#"
            CREATE TABLE mail_conversation_scroll_data (
                local_label_id INTEGER NOT NULL,
                unread INTEGER NOT NULL DEFAULT 0,
                remote_conversation_id TEXT NOT NULL,
                conversation_time INTEGER NOT NULL,
                display_order INTEGER NOT NULL,
                PRIMARY KEY (local_label_id, unread)
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
                PRIMARY KEY (local_label_id, unread)
            )
        "#,
        vec![],
    )
    .await?;

    Ok(())
}
