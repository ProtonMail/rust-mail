use futures::executor::block_on;
use stash::stash::{StashError, Tether};

pub fn create_tables(tx: &Tether) -> Result<(), StashError> {
    block_on(async {
        tx.execute(
            r"
            CREATE TABLE contacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                remote_id TEXT UNIQUE,
                name TEXT NOT NULL,
                uid TEXT NOT NULL,
                size INTEGER NOT NULL,
                create_time INTEGER NOT NULL,
                modify_time INTEGER NOT NULL
            )
        ",
            vec![],
        )
        .await?;

        tx.execute(
            r"CREATE UNIQUE INDEX index_contact_remote_id ON contacts (remote_id)",
            vec![],
        )
        .await?;

        tx.execute(
            r"
            CREATE TABLE contact_emails (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                remote_id TEXT UNIQUE,
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                defaults INTEGER NOT NULL,
                display_order INTEGER NOT NULL,
                contact_id INTEGER NOT NULL,
                remote_contact_id TEXT,
                canonical_email TEXT NOT NULL,
                last_used_time INTEGER NOT NULL,
                is_proton INTEGER NOT NULL,

                CONSTRAINT constraint_contact_emails_cid
                    FOREIGN KEY (contact_id)
                    REFERENCES contacts (id)
                    ON DELETE CASCADE
            )
        ",
            vec![],
        )
        .await?;

        tx.execute(
            r"CREATE INDEX index_contact_emails_email ON contact_emails (canonical_email)",
            vec![],
        )
        .await?;

        tx.execute(
            r"CREATE INDEX index_contact_emails_contact_id ON contact_emails (contact_id)",
            vec![],
        )
        .await?;

        tx.execute(
            r"
            CREATE TABLE contact_cards (
                contact_id INTEGER NOT NULL,
                card_type INTEGER NOT NULL,
                data TEXT NOT NULL,
                signature TEXT,

                PRIMARY KEY(contact_id, data),

                CONSTRAINT constraint_contact_cards_cid
                   FOREIGN KEY (contact_id)
                   REFERENCES contacts (id)
                   ON DELETE CASCADE
            )
        ",
            vec![],
        )
        .await?;

        tx.execute(
            r"CREATE INDEX index_contact_cards_id ON contact_cards (contact_id)",
            vec![],
        )
        .await?;

        tx.execute(
            r"
            CREATE TABLE contact_email_labels (
                contact_emails_id INTEGER NOT NULL,
                value TEXT NOT NULL,

                PRIMARY KEY(contact_emails_id, value),

                CONSTRAINT constraint_contact_label_cid
                    FOREIGN KEY (contact_emails_id)
                    REFERENCES contact_emails (id)
                    ON DELETE CASCADE
            )
        ",
            vec![],
        )
        .await?;

        tx.execute(
        r"CREATE INDEX index_contact_email_label_id ON contact_email_labels (contact_emails_id)",
        vec![],
    )
    .await?;

        Ok(())
    })
}
