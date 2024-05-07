use crate::db::{
    DBResult, LabelColor, LocalLabel, LocalLabelId, LocalLabelWithCount, MailSqliteConnectionImpl,
};
use proton_api_mail::domain::{Label, LabelId, LabelType};
pub use proton_api_mail::proton_api_core::exports::serde_json;
use proton_sqlite3::rusqlite::{params_from_iter, OptionalExtension, Row};
use proton_sqlite3::utils;
use utils::{gen_variable_in_argument_list, mapped_rows_into_vec};

// --------- LOCAL Labels -----------------------------------------------------------------------

pub(crate) fn movable_sys_folder_list() -> [&'static LabelId; 4] {
    [
        LabelId::inbox(),
        LabelId::archive(),
        LabelId::spam(),
        LabelId::trash(),
    ]
}

impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn labels_with_ids(
        &self,
        ids: impl ExactSizeIterator<Item = LocalLabelId>,
    ) -> DBResult<Vec<LocalLabel>> {
        let mut result = Vec::with_capacity(ids.len());
        let query = LocalLabelSelect::query_in(ids.len());
        let mut stmt = self.0.prepare(&query)?;
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map(params_from_iter(ids), LocalLabelSelect::from_row)?,
        )?;
        Ok(result)
    }

    pub fn labels(&self) -> DBResult<Vec<LocalLabel>> {
        let mut result = Vec::with_capacity(8);
        let mut stmt = self.0.prepare(LocalLabelSelect::query_all())?;
        mapped_rows_into_vec(&mut result, stmt.query_map((), LocalLabelSelect::from_row)?)?;
        Ok(result)
    }

    pub fn label_by_type_ordered(&self, label_type: LabelType) -> DBResult<Vec<LocalLabel>> {
        let mut result = Vec::with_capacity(8);
        let mut stmt = self.0.prepare(LocalLabelSelect::query_by_type_ordered())?;
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map([label_type], LocalLabelSelect::from_row)?,
        )?;
        Ok(result)
    }

    pub fn label_by_type_ordered_with_conversation_count(
        &self,
        label_type: LabelType,
    ) -> DBResult<Vec<LocalLabelWithCount>> {
        let mut result = Vec::with_capacity(8);
        let mut stmt = self
            .0
            .prepare(LocalLabelSelectWithCount::query_conversation())?;
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map([label_type], LocalLabelSelectWithCount::from_row)?,
        )?;
        Ok(result)
    }

    pub fn label_by_type_ordered_with_message_count(
        &self,
        label_type: LabelType,
    ) -> DBResult<Vec<LocalLabelWithCount>> {
        let mut result = Vec::with_capacity(8);
        let mut stmt = self.0.prepare(LocalLabelSelectWithCount::query_message())?;
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map([label_type], LocalLabelSelectWithCount::from_row)?,
        )?;
        Ok(result)
    }

    pub fn label_with_id(&self, local_label_id: LocalLabelId) -> DBResult<Option<LocalLabel>> {
        self.0
            .query_row(
                &LocalLabelSelect::query_with_id(),
                [local_label_id],
                LocalLabelSelect::from_row,
            )
            .optional()
    }

    /// Get label with a given local id or fail.
    pub fn label_with_id_or_err(&self, local_label_id: LocalLabelId) -> DBResult<LocalLabel> {
        self.0.query_row(
            &LocalLabelSelect::query_with_id(),
            [local_label_id],
            LocalLabelSelect::from_row,
        )
    }

    pub fn label_with_remote_id(&self, label_id: &LabelId) -> DBResult<Option<LocalLabel>> {
        self.0
            .query_row(
                &LocalLabelSelect::query_with_rid(),
                [label_id],
                LocalLabelSelect::from_row,
            )
            .optional()
    }

    pub fn create_label(
        &mut self,
        label_type: LabelType,
        name: String,
        parent_id: Option<LocalLabelId>,
        path: Option<String>,
        color: LabelColor,
    ) -> DBResult<LocalLabel> {
        let notified = label_type == LabelType::Folder;
        let expanded = label_type == LabelType::Folder;
        let (id, inserted_order) : (LocalLabelId, u32) = self.0.query_row(
            "INSERT INTO labels (type, parent_id, name, path, color, `order`, notified, expanded) VALUES \
(?1,?2,?3,?4,?5,(SELECT ifnull(MAX(`order`)+1, 0) FROM labels WHERE type=?1),?6,?7) RETURNING id, `order`",
            (label_type, &parent_id, &name, &path, &color, notified, expanded),
            |r| Ok((r.get(0)?, r.get(1)?))
        )?;

        Ok(LocalLabel {
            id,
            rid: None,
            parent_id,
            name,
            path,
            color,
            label_type,
            order: inserted_order,
            notify: notified,
            expanded,
            sticky: false,
        })
    }

    pub fn update_label_name(&mut self, label_id: LocalLabelId, name: &str) -> DBResult<()> {
        self.0
            .execute("UPDATE labels SET name=? WHERE id=?", (name, label_id))?;
        Ok(())
    }

    pub fn update_label_color(
        &mut self,
        label_id: LocalLabelId,
        color: &LabelColor,
    ) -> DBResult<()> {
        self.0
            .execute("UPDATE labels SET color=? WHERE id=?", (color, label_id))?;
        Ok(())
    }

    pub fn update_label_parent(
        &mut self,
        label_id: LocalLabelId,
        parent_id: Option<LocalLabelId>,
        path: Option<&str>,
    ) -> DBResult<()> {
        self.0.execute(
            "UPDATE labels SET parent_id=?, path=? WHERE id=?",
            (parent_id, path, label_id),
        )?;
        Ok(())
    }

    pub fn mark_labels_as_deleted(
        &mut self,
        deleted: bool,
        ids: impl Iterator<Item = LocalLabelId>,
    ) -> DBResult<()> {
        let mut stmt = self
            .0
            .prepare("UPDATE labels SET deleted =? WHERE id = ?")?;
        for id in ids {
            stmt.execute((deleted, id))?;
        }
        Ok(())
    }

    pub fn mark_label_as_deleted(&mut self, deleted: bool, id: LocalLabelId) -> DBResult<()> {
        self.mark_labels_as_deleted(deleted, std::iter::once(id))
    }

    /// Check if a label's conversations have been initialised.
    ///
    /// This simply checks whether a given label's conversations have been marked as initialised.
    /// For more information on the behaviour of this flag, see
    /// [`mark_label_as_initialized_conversations()`](MailSqliteConnectionImpl::mark_label_as_initialized_conversations()).
    ///
    /// # Parameters
    ///
    /// * `id` - The id of the label to check.
    ///
    /// # Errors
    ///
    /// If the database operation fails, the error will be returned unmodified.
    ///
    /// # See also
    ///
    /// * [`mark_label_as_initialized_conversations()`](MailSqliteConnectionImpl::mark_label_as_initialized_conversations())
    ///
    pub fn check_if_label_is_initialized_conversations(&self, id: LocalLabelId) -> DBResult<bool> {
        self.0
            .prepare("SELECT 1 FROM labels WHERE id = ? AND initialized_conv = 1")?
            .query([id])?
            .next()
            .map(|r| r.is_some())
    }

    /// Mark a label's conversations as initialised.
    ///
    /// This is used to mark a label that has been initialised — in other words,
    /// which has had its initial load of conversations. It is undesirable to repeat the
    /// initial data load if we already have it, hence this flag. Once a label
    /// has been marked as initialised, the initial data load will not be
    /// repeated.
    ///
    /// # Parameters
    ///
    /// * `id` - The id of the label to mark as initialised.
    ///
    /// # Errors
    ///
    /// If the database operation fails, the error will be returned unmodified.
    ///
    /// # See also
    ///
    /// * [`check_if_label_is_initialized_conversations()`](MailSqliteConnectionImpl::check_if_label_is_initialized_conversations())
    ///
    pub fn mark_label_as_initialized_conversations(&mut self, id: LocalLabelId) -> DBResult<()> {
        self.0
            .prepare("UPDATE labels SET initialized_conv = 1 WHERE id = ?")?
            .execute([id])?;
        Ok(())
    }

    /// Check if a label's messages have been initialised.
    ///
    /// This simply checks whether a given label's messages have been marked as initialised.
    /// For more information on the behaviour of this flag, see
    /// [`mark_label_as_initialized_messages()`](MailSqliteConnectionImpl::mark_label_as_initialized_messages()).
    ///
    /// # Parameters
    ///
    /// * `id` - The id of the label to check.
    ///
    /// # Errors
    ///
    /// If the database operation fails, the error will be returned unmodified.
    ///
    /// # See also
    ///
    /// * [`mark_label_as_initialized_messages()`](MailSqliteConnectionImpl::mark_label_as_initialized_messages())
    ///
    pub fn check_if_label_is_initialized_messages(&self, id: LocalLabelId) -> DBResult<bool> {
        self.0
            .prepare("SELECT 1 FROM labels WHERE id = ? AND initialized_msg = 1")?
            .query([id])?
            .next()
            .map(|r| r.is_some())
    }

    /// Mark a label's messages as initialised.
    ///
    /// This is used to mark a label that has been initialised — in other words,
    /// which has had its initial load of messages. It is undesirable to repeat the
    /// initial data load if we already have it, hence this flag. Once a label
    /// has been marked as initialised, the initial data load will not be
    /// repeated.
    ///
    /// # Parameters
    ///
    /// * `id` - The id of the label to mark as initialised.
    ///
    /// # Errors
    ///
    /// If the database operation fails, the error will be returned unmodified.
    ///
    /// # See also
    ///
    /// * [`check_if_label_is_initialized_messages()`](MailSqliteConnectionImpl::check_if_label_is_initialized_messages())
    ///
    pub fn mark_label_as_initialized_messages(&mut self, id: LocalLabelId) -> DBResult<()> {
        self.0
            .prepare("UPDATE labels SET initialized_msg = 1 WHERE id = ?")?
            .execute([id])?;
        Ok(())
    }

    /// Return the list of labels that are valid folders for a conversation or message to be moved into.
    pub fn labels_for_conv_or_msg_move(&self) -> DBResult<Vec<LocalLabel>> {
        let mut folders = self.label_by_type_ordered(LabelType::Folder)?;

        // Get the system labels.
        let sys_folders = movable_sys_folder_list();
        let mut stmt = self
            .0
            .prepare(&LocalLabelSelect::query_in_rid(sys_folders.len()))?;

        mapped_rows_into_vec(
            &mut folders,
            stmt.query_map(params_from_iter(sys_folders), LocalLabelSelect::from_row)?,
        )?;

        Ok(folders)
    }
}

struct LocalLabelSelect {}

impl LocalLabelSelect {
    fn query_all() -> &'static str {
        "SELECT id, rid, parent_id, type, `order`, name, path, color, notified, expanded, sticky FROM labels WHERE deleted=0"
    }

    fn query_by_type_ordered() -> &'static str {
        "SELECT id, rid, parent_id, type, `order`, name, path, color, notified, expanded, sticky FROM labels WHERE deleted=0 AND type=? ORDER BY `order`"
    }

    fn query_with_id() -> String {
        format!("{} AND id = ?", Self::query_all())
    }

    fn query_with_rid() -> String {
        format!("{} AND rid = ?", Self::query_all())
    }
    fn query_in(count: usize) -> String {
        format!(
            "{} AND id IN ({})",
            Self::query_all(),
            gen_variable_in_argument_list(count)
        )
    }
    fn query_in_rid(count: usize) -> String {
        format!(
            "{} AND rid IN ({})",
            Self::query_all(),
            gen_variable_in_argument_list(count)
        )
    }

    fn from_row(r: &Row) -> DBResult<LocalLabel> {
        Ok(LocalLabel {
            id: r.get(0)?,
            rid: r.get(1)?,
            parent_id: r.get(2)?,
            label_type: r.get(3)?,
            order: r.get(4)?,
            name: r.get(5)?,
            path: r.get(6)?,
            color: r.get(7)?,
            notify: r.get(8)?,
            expanded: r.get(9)?,
            sticky: r.get(10)?,
        })
    }
}

struct LocalLabelSelectWithCount {}

impl LocalLabelSelectWithCount {
    fn query_conversation() -> &'static str {
        "SELECT l.id, l.rid, l.parent_id, l.type, l.`order`, l.name, l.path, l.color, l.notified, \
        l.expanded, l.sticky, IFNULL(lc.total,0), IFNULL(lc.unread,0) FROM labels as l \
        LEFT JOIN label_conversation_count AS lc ON l.id = lc.label_id \
        WHERE deleted=0 AND type=? ORDER BY `order`"
    }
    fn query_message() -> &'static str {
        "SELECT l.id, l.rid, l.parent_id, l.type, l.`order`, l.name, l.path, l.color, l.notified, \
        l.expanded, l.sticky, IFNULL(lc.total,0), IFNULL(lc.unread,0) FROM labels as l \
        LEFT JOIN label_message_count AS lc ON l.id = lc.label_id \
        WHERE deleted=0 AND type=? ORDER BY `order`"
    }

    fn from_row(r: &Row) -> DBResult<LocalLabelWithCount> {
        Ok(LocalLabelWithCount {
            id: r.get(0)?,
            rid: r.get(1)?,
            parent_id: r.get(2)?,
            label_type: r.get(3)?,
            order: r.get(4)?,
            name: r.get(5)?,
            path: r.get(6)?,
            color: r.get(7)?,
            notified: r.get(8)?,
            expanded: r.get(9)?,
            sticky: r.get(10)?,
            total_count: r.get(11)?,
            unread_count: r.get(12)?,
        })
    }
}

// --------- REMOTE Labels -----------------------------------------------------------------------
// NOTE: Local values are update via triggers when remote data changes, since remote always wins.
impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn create_remote_labels<'i>(
        &mut self,
        labels: impl ExactSizeIterator<Item = &'i Label>,
    ) -> DBResult<Vec<LocalLabelId>> {
        let mut label_ids = Vec::with_capacity(labels.len());
        let mut stmt_remote = self.0.prepare(
            r"INSERT INTO labels (
rid, parent_id, type, `order`, name, path, color, notified, expanded, sticky)
VALUES (?,(SELECT id FROM labels WHERE rid=?),?,?,?,?,?,?,?,?)
ON CONFLICT (rid) DO UPDATE SET `order`=excluded.`order`, name=excluded.name, path=excluded.path,
color=excluded.color, parent_id=excluded.parent_id RETURNING id",
        )?;
        for label in labels {
            let sticky: bool = label.sticky;
            let expanded: bool = label.expanded;
            let notify: bool = label.notify;
            let local_id = stmt_remote.query_row(
                (
                    &label.id,
                    &label.parent_id,
                    label.label_type,
                    label.order,
                    &label.name,
                    &label.path,
                    &label.color,
                    notify,
                    expanded,
                    sticky,
                ),
                |r| r.get(0),
            )?;

            label_ids.push(local_id)
        }
        Ok(label_ids)
    }

    pub fn create_remote_label(&mut self, label: &Label) -> DBResult<LocalLabelId> {
        Ok(self.create_remote_labels(std::iter::once(label))?[0])
    }
    pub fn update_remote_labels<'i>(
        &mut self,
        labels: impl Iterator<Item = &'i Label>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(
            "UPDATE labels SET parent_id=(SELECT id FROM labels WHERE rid=?), \
`order`=?, name=?, path=?, color=?, \
expanded=?, notified=?, sticky=? WHERE rid=?",
        )?;
        for label in labels {
            let sticky: bool = label.sticky;
            let expanded: bool = label.expanded;
            let notify: bool = label.notify;
            stmt.execute((
                &label.parent_id,
                label.order,
                &label.name,
                &label.path,
                &label.color,
                expanded,
                notify,
                sticky,
                &label.id,
            ))?;
        }
        Ok(())
    }

    pub fn update_remote_label(&mut self, label: &Label) -> DBResult<()> {
        self.update_remote_labels(std::iter::once(label))
    }

    pub fn delete_remote_labels<'i>(
        &mut self,
        ids: impl Iterator<Item = &'i LabelId>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare("DELETE FROM labels WHERE rid=?")?;
        for id in ids {
            stmt.execute([id])?;
        }
        Ok(())
    }

    pub fn delete_remote_label(&mut self, id: &LabelId) -> DBResult<()> {
        self.delete_remote_labels(std::iter::once(id))
    }

    pub fn resolve_remote_label_id(&self, id: &LabelId) -> DBResult<Option<LocalLabelId>> {
        self.0
            .query_row("SELECT id FROM labels WHERE rid=?", [id], |r| r.get(0))
            .optional()
    }

    /// Get the remote id for a label with `id`.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn remote_label_id_from_local_id(
        &self,
        id: LocalLabelId,
    ) -> DBResult<Option<Option<LabelId>>> {
        self.0
            .query_row("SELECT rid FROM labels WHERE id=?", [id], |r| r.get(0))
            .optional()
    }

    pub fn resolve_remote_label_ids<'i>(
        &self,
        ids: impl ExactSizeIterator<Item = &'i LabelId>,
    ) -> DBResult<Vec<LocalLabelId>> {
        debug_assert!(ids.len() < 500);
        let mut stmt = self.0.prepare(&format!(
            "SELECT id FROM labels WHERE rid IN ({})",
            gen_variable_in_argument_list(ids.len())
        ))?;
        let mut result = Vec::with_capacity(ids.len());
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map(params_from_iter(ids), |r| r.get(0))?,
        )?;
        Ok(result)
    }
}
