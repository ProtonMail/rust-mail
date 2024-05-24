use crate::db::contacts::{LocalContactEmailId, LocalContactId};
use crate::db::{CoreSqliteConnectionImpl, DBResult};
use crate::json::deserialize_json_from_row;
use proton_api_core::domain::{Contact, ContactCard, ContactEmail, ContactLabelId, ContactPartial};
use proton_sqlite3::rusqlite::{OptionalExtension, Row, Statement};
use proton_sqlite3::utils::mapped_rows_to_vec;
use proton_sqlite3::{bind_list_indexed, bind_list_indexed_recursive};

use super::{LocalContact, LocalContactCard, LocalContactEmail, LocalContactWithCards};

impl<'c> CoreSqliteConnectionImpl<'c> {
    /// Updates the contact in the database with its emails and v-cards.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn create_or_update_contact(&mut self, contact: &Contact) -> DBResult<LocalContactId> {
        let mut insert_contact_stmt = self.prepare_sql_statement_insert_contact()?;
        let mut insert_card_stmt = self.prepare_sql_statement_insert_card()?;
        let mut insert_email_stmt = self.prepare_sql_statement_contact_insert_mail()?;
        let mut insert_email_label_stmt = self.prepare_sql_statement_contact_mail_insert_label()?;

        // Insert or update the contact.
        bind_list_indexed!(
            &mut insert_contact_stmt,
            &contact.id,
            &contact.name,
            &contact.uid,
            contact.size,
            contact.create_time,
            contact.modify_time,
        );
        let local_id: LocalContactId = insert_contact_stmt
            .raw_query()
            .next()?
            .ok_or(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows)
            .and_then(|r| r.get(0))?;

        // Insert or update the contact's emails.
        for contact_email in &contact.contact_emails {
            bind_list_indexed!(
                &mut insert_email_stmt,
                &contact_email.id,
                &contact_email.name,
                &contact_email.email,
                contact_email.defaults,
                contact_email.order,
                local_id,
                &contact.id,
                &contact_email.canonical_email,
                contact_email.last_used_time,
                contact_email.is_proton,
            );
            let local_email_id: LocalContactEmailId = insert_email_stmt
                .raw_query()
                .next()?
                .ok_or(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows)
                .and_then(|r| r.get(0))?;
            for label_id in &contact_email.label_ids {
                insert_email_label_stmt.execute((local_email_id, label_id))?;
            }
        }
        // Insert or update the contact's cards.
        for card in &contact.cards {
            insert_card_stmt.execute((local_id, card.card_type, &card.data, &card.signature))?;
        }

        Ok(local_id)
    }

    /// Updates the multiple contacts in the database with its emails and v-cards.
    ///
    /// # Errors
    /// Returns an error if one of the DB transaction fails.
    pub fn create_or_update_contacts<'i>(
        &mut self,
        contacts: impl Iterator<Item = &'i Contact>,
    ) -> DBResult<Vec<LocalContactId>> {
        let mut ids = Vec::new();
        for contact in contacts {
            ids.push(self.create_or_update_contact(contact)?);
        }
        Ok(ids)
    }

    /// Updates the contacts in the database without its emails and v-cards.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn create_or_update_partial_contacts<'i>(
        &mut self,
        contacts: impl Iterator<Item = &'i ContactPartial>,
    ) -> DBResult<Vec<LocalContactId>> {
        let mut insert_contact_stmt = self.prepare_sql_statement_insert_contact()?;
        let mut local_ids = Vec::new();
        // Insert or update the partial contacts.
        for contact in contacts {
            bind_list_indexed!(
                &mut insert_contact_stmt,
                &contact.id,
                &contact.name,
                &contact.uid,
                contact.size,
                contact.create_time,
                contact.modify_time,
            );
            let local_id: LocalContactId = insert_contact_stmt
                .raw_query()
                .next()?
                .ok_or(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows)
                .and_then(|r| r.get(0))?;
            local_ids.push(local_id);
        }
        Ok(local_ids)
    }

    /// Updates the contact emails in the database.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn create_or_update_contact_emails<'i>(
        &mut self,
        contact_emails: impl Iterator<Item = &'i ContactEmail>,
    ) -> DBResult<Vec<LocalContactEmailId>> {
        let mut insert_contact_mail_stmt =
            self.prepare_sql_statement_contact_insert_mail_with_contact_rid()?;
        let mut insert_email_label_stmt = self.prepare_sql_statement_contact_mail_insert_label()?;
        let mut local_ids = Vec::new();
        // Insert or update the email contacts.
        for contact_email in contact_emails {
            bind_list_indexed!(
                &mut insert_contact_mail_stmt,
                &contact_email.id,
                &contact_email.name,
                &contact_email.email,
                contact_email.defaults,
                contact_email.order,
                &contact_email.contact_id,
                &contact_email.contact_id,
                &contact_email.canonical_email,
                contact_email.last_used_time,
                contact_email.is_proton,
            );
            let local_id: LocalContactEmailId = insert_contact_mail_stmt
                .raw_query()
                .next()?
                .ok_or(proton_sqlite3::rusqlite::Error::QueryReturnedNoRows)
                .and_then(|r| r.get(0))?;
            local_ids.push(local_id);
            for label_id in &contact_email.label_ids {
                insert_email_label_stmt.execute((local_id, label_id))?;
            }
        }
        Ok(local_ids)
    }

    /// Queries teh database for the contact emails matching the provided email.
    ///
    /// The the provided email must be in canonical form.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn query_contact_emails_by_mail(
        &self,
        canonical_email: &str,
    ) -> DBResult<Vec<LocalContactEmail>> {
        let mut query_statement = self.0.prepare(&ContactMailSelector::query_with_email())?;
        query_statement.raw_bind_parameter(1, canonical_email)?;
        let rows = query_statement
            .raw_query()
            .mapped(ContactMailSelector::from_row);
        mapped_rows_to_vec(rows)
    }

    /// Queries all contact emails for the user.
    ///
    /// The number of contacts emails is limited by the `limit` parameter while
    /// the `offset` determines the offset to query from.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn query_contact_emails(
        &self,
        offset: u64,
        limit: u64,
    ) -> DBResult<Vec<LocalContactEmail>> {
        let mut query_statement = self
            .0
            .prepare(&ContactMailSelector::query_all_with_limit())?;
        query_statement.raw_bind_parameter(1, limit)?;
        query_statement.raw_bind_parameter(2, offset)?;
        let rows = query_statement
            .raw_query()
            .mapped(ContactMailSelector::from_row);
        mapped_rows_to_vec(rows)
    }

    /// Queries all contacts with its emails but no v-cards.
    ///
    /// The number of contacts is limited by the `limit` parameter while
    /// the `offset` determines the offset to query from.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn query_contacts(&self, offset: u64, limit: u64) -> DBResult<Vec<LocalContact>> {
        let mut query_statement = self.0.prepare(&ContactSelector::query_all_with_limit())?;
        query_statement.raw_bind_parameter(1, limit)?;
        query_statement.raw_bind_parameter(2, offset)?;
        let rows = query_statement
            .raw_query()
            .mapped(ContactSelector::from_row);
        mapped_rows_to_vec(rows)
    }

    /// Queries a single contact with its emails but no v-cards.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn query_contact(&self, contact_id: LocalContactId) -> DBResult<Option<LocalContact>> {
        self.0
            .query_row(
                &ContactSelector::query_single_with_id(),
                [contact_id],
                ContactSelector::from_row,
            )
            .optional()
    }

    /// Queries a single contact with its emails including its cards.
    ///
    /// # Errors
    /// Returns an error if the DB transaction fails.
    pub fn query_contact_with_cards(
        &self,
        contact_id: LocalContactId,
    ) -> DBResult<Option<LocalContactWithCards>> {
        self.0
            .query_row(
                &ContactWithCardsSelector::query_single_with_id(),
                [contact_id],
                ContactWithCardsSelector::from_row,
            )
            .optional()
    }
}

impl<'c> CoreSqliteConnectionImpl<'c> {
    fn prepare_sql_statement_insert_contact(&self) -> DBResult<Statement> {
        const INSERT_CONTACT_SQL: &str = r"
            INSERT INTO contacts (
                rid, 
                name, 
                uid, 
                size, 
                create_time, 
                modify_time 
            ) VALUES (?,?,?,?,?,?)
            ON CONFLICT (rid) DO UPDATE SET
                id=id
            RETURNING id";
        self.0.prepare(INSERT_CONTACT_SQL)
    }

    fn prepare_sql_statement_contact_insert_mail(&self) -> DBResult<Statement> {
        const INSERT_EMAIL_SQL: &str = r"
            INSERT INTO contact_emails (
                rid, 
                name, 
                email, 
                defaults, 
                `order`, 
                contact_id, 
                remote_contact_id,
                canonical_email, 
                last_used_time, 
                is_proton
            ) VALUES (?,?,?,?,?,?,?,?,?,?)
            ON CONFLICT (rid) DO UPDATE SET
                id=id,
                contact_id=excluded.contact_id
            RETURNING id";
        self.0.prepare(INSERT_EMAIL_SQL)
    }

    fn prepare_sql_statement_contact_insert_mail_with_contact_rid(&self) -> DBResult<Statement> {
        const INSERT_EMAIL_SQL: &str = r"
            INSERT INTO contact_emails (
                rid, 
                name, 
                email, 
                defaults, 
                `order`, 
                contact_id,
                remote_contact_id,
                canonical_email, 
                last_used_time, 
                is_proton
            ) VALUES (?,?,?,?,?,(SELECT id FROM contacts WHERE contacts.rid = ?),?,?,?,?)
            ON CONFLICT (rid) DO UPDATE SET
                id=id,
                contact_id=excluded.contact_id
            RETURNING id";
        self.0.prepare(INSERT_EMAIL_SQL)
    }

    fn prepare_sql_statement_insert_card(&self) -> DBResult<Statement> {
        const INSERT_CARD_SQL: &str = r"
            INSERT OR REPLACE INTO contact_cards (
                contact_id, 
                card_type, 
                data, 
                signature
            ) VALUES (?,?,?,?)";
        self.0.prepare(INSERT_CARD_SQL)
    }

    fn prepare_sql_statement_contact_mail_insert_label(&self) -> DBResult<Statement> {
        const INSERT_EMAIL_LABEL_SQL: &str = r"
            INSERT OR REPLACE INTO contact_email_labels (
                contact_emails_id, 
                value
            ) VALUES (?,?)";
        self.0.prepare(INSERT_EMAIL_LABEL_SQL)
    }
}

struct ContactMailSelector {}

impl ContactMailSelector {
    const QUERY_PREFIX: &'static str = r"
        WITH 
            json_contact_mail_labels AS (
                SELECT
                    C.contact_emails_id as ceid,
                    json_group_array(
                        C.value
                    ) as labels
                FROM contact_email_labels as C
                GROUP BY C.contact_emails_id
            )
        SELECT
            C.id,
            C.rid,
            C.name, 
            C.email, 
            C.defaults, 
            C.`order`, 
            C.contact_id,
            C.remote_contact_id, 
            C.canonical_email, 
            C.last_used_time, 
            C.is_proton,
            CML.labels
        FROM contact_emails AS C
        LEFT JOIN json_contact_mail_labels AS CML ON CML.ceid = C.id
        ";

    fn query_with_email() -> String {
        format!(r"{}WHERE C.canonical_email = ?", Self::QUERY_PREFIX)
    }

    fn query_all_with_limit() -> String {
        format!("{}LIMIT ? OFFSET ?", Self::QUERY_PREFIX)
    }

    fn from_row(r: &Row) -> DBResult<LocalContactEmail> {
        Ok({
            LocalContactEmail {
                id: r.get(0)?,
                rid: r.get(1)?,
                name: r.get(2)?,
                email: r.get(3)?,
                defaults: r.get(4)?,
                order: r.get(5)?,
                contact_id: r.get(6)?,
                remote_contact_id: r.get(7)?,
                canonical_email: r.get(8)?,
                last_used_time: r.get(9)?,
                is_proton: r.get(10)?,
                contact_labels: deserialize_json_from_row::<Vec<ContactLabelId>>(r, 11)?,
            }
        })
    }
}

struct ContactSelector {}

impl ContactSelector {
    const QUERY_PREFIX: &'static str = r"
        WITH 
            json_contact_mails AS (
                SELECT
                    C.contact_id AS cid,
                    json_group_array(
                        json_object(
                            'id', C.id,
                            'rid', C.rid,
                            'name', C.name,
                            'email', C.email,
                            'defaults', C.defaults,
                            'order', C.`order`,
                            'contact_id', C.contact_id,
                            'remote_contact_id', C.remote_contact_id,
                            'canonical_email', C.canonical_email,
                            'last_used_time', C.last_used_time,
                            'is_proton', C.is_proton,
                            'contact_labels', json(CML.labels)
                        )
                    ) as json_mails
                FROM contact_emails as C
                LEFT JOIN (
                    SELECT
                        CE.contact_emails_id as ceid,
                        json_group_array(
                            CE.value
                        ) as labels
                    FROM contact_email_labels as CE
                    GROUP BY CE.contact_emails_id
                ) AS CML ON CML.ceid = C.id
                GROUP BY C.contact_id
            )
        SELECT
            C.id,
            C.rid,
            C.name,
            C.uid,
            C.size,
            C.create_time,
            C.modify_time,
            CM.json_mails
        FROM contacts AS C
        LEFT JOIN json_contact_mails AS CM ON CM.cid = C.id
        ";

    fn query_all_with_limit() -> String {
        format!("{}LIMIT ? OFFSET ?", Self::QUERY_PREFIX)
    }

    fn query_single_with_id() -> String {
        format!("{}WHERE C.id=?", Self::QUERY_PREFIX)
    }
    fn from_row(r: &Row) -> DBResult<LocalContact> {
        Ok({
            LocalContact {
                id: r.get(0)?,
                rid: r.get(1)?,
                name: r.get(2)?,
                uid: r.get(3)?,
                size: r.get(4)?,
                create_time: r.get(5)?,
                modify_time: r.get(6)?,
                contact_emails: deserialize_json_from_row::<Vec<LocalContactEmail>>(r, 7)?,
            }
        })
    }
}

struct ContactWithCardsSelector {}

impl ContactWithCardsSelector {
    const QUERY_PREFIX: &'static str = r"
        WITH 
            json_contact_mails AS (
                SELECT
                    C.contact_id AS cid,
                    json_group_array(
                        json_object(
                            'id', C.id,
                            'rid', C.rid,
                            'name', C.name,
                            'email', C.email,
                            'defaults', C.defaults,
                            'order', C.`order`,
                            'contact_id', C.contact_id,
                            'remote_contact_id', C.remote_contact_id,
                            'canonical_email', C.canonical_email,
                            'last_used_time', C.last_used_time,
                            'is_proton', C.is_proton,
                            'contact_labels', json(CML.labels)
                        )
                    ) as json_mails
                FROM contact_emails as C
                LEFT JOIN (
                    SELECT
                        CE.contact_emails_id as ceid,
                        json_group_array(
                            CE.value
                        ) as labels
                    FROM contact_email_labels as CE
                    GROUP BY CE.contact_emails_id
                ) AS CML ON CML.ceid = C.id
            GROUP BY C.contact_id
            ),
            json_contact_cards AS (
                SELECT
                    CC.contact_id as cid,
                    json_group_array(
                        json_object(
                            'Type', CC.card_type,
                            'Data', CC.data,
                            'Signature', CC.signature
                        )
                    ) as json_cards
                FROM contact_cards AS CC
                GROUP BY CC.contact_id
            )
        SELECT
            C.id,
            C.rid,
            C.name,
            C.uid,
            C.size,
            C.create_time,
            C.modify_time,
            CM.json_mails,
            CA.json_cards
        FROM contacts AS C
        LEFT JOIN json_contact_mails AS CM ON CM.cid = C.id
        LEFT JOIN json_contact_cards AS CA ON CA.cid = C.id
        ";

    fn query_single_with_id() -> String {
        format!("{}WHERE C.id=?", Self::QUERY_PREFIX)
    }

    fn from_row(r: &Row) -> DBResult<LocalContactWithCards> {
        let local_contact = LocalContact {
            id: r.get(0)?,
            rid: r.get(1)?,
            name: r.get(2)?,
            uid: r.get(3)?,
            size: r.get(4)?,
            create_time: r.get(5)?,
            modify_time: r.get(6)?,
            contact_emails: deserialize_json_from_row::<Vec<LocalContactEmail>>(r, 7)?,
        };
        let cards_raw = deserialize_json_from_row::<Vec<ContactCard>>(r, 8)?;
        let mut cards: Vec<LocalContactCard> = Vec::with_capacity(cards_raw.len());
        cards.extend(cards_raw.into_iter().map(Into::into));
        Ok({
            LocalContactWithCards {
                local_contact,
                cards,
            }
        })
    }
}
