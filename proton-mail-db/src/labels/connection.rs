use crate::{
    DBResult, DeletedState, LabelColor, LocalLabel, LocalLabelId, MailSqliteConnectionImpl,
    RemoteLabel,
};
use proton_api_mail::domain::{Label, LabelId, LabelType};
pub use proton_api_mail::proton_api_core::exports::serde_json;
use proton_sqlite3::rusqlite::{params_from_iter, OptionalExtension, Row};
use proton_sqlite3::utils;
use utils::{gen_variable_in_argument_list, mapped_rows_into_vec};

// --------- LOCAL Labels -----------------------------------------------------------------------

impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn get_local_labels<'i>(
        &self,
        ids: impl ExactSizeIterator<Item = &'i LabelId>,
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

    pub fn get_all_local_labels(&self) -> DBResult<Vec<LocalLabel>> {
        let mut result = Vec::with_capacity(8);
        let mut stmt = self.0.prepare(LocalLabelSelect::query_all())?;
        mapped_rows_into_vec(&mut result, stmt.query_map((), LocalLabelSelect::from_row)?)?;
        Ok(result)
    }

    pub fn get_local_label_by_type_ordered(
        &self,
        label_type: LabelType,
    ) -> DBResult<Vec<LocalLabel>> {
        let mut result = Vec::with_capacity(8);
        let mut stmt = self.0.prepare(LocalLabelSelect::query_by_type_ordered())?;
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map([label_type], LocalLabelSelect::from_row)?,
        )?;
        Ok(result)
    }

    pub fn get_local_label(&self, local_label_id: LocalLabelId) -> DBResult<Option<LocalLabel>> {
        self.0
            .query_row(
                &LocalLabelSelect::query_with_id(),
                [local_label_id],
                LocalLabelSelect::from_row,
            )
            .optional()
    }

    pub fn create_local_label(
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
            notified,
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
        deleted_state: DeletedState,
        ids: impl Iterator<Item = LocalLabelId>,
    ) -> DBResult<()> {
        let mut stmt = self
            .0
            .prepare("UPDATE labels SET deleted =? WHERE id = ?")?;
        for id in ids {
            stmt.execute((deleted_state, id))?;
        }
        Ok(())
    }

    pub fn mark_label_as_deleted(
        &mut self,
        deleted_state: DeletedState,
        id: LocalLabelId,
    ) -> DBResult<()> {
        self.mark_labels_as_deleted(deleted_state, std::iter::once(id))
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
    fn query_in(count: usize) -> String {
        format!(
            "{} AND id IN ({})",
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
            notified: r.get(8)?,
            expanded: r.get(9)?,
            sticky: r.get(10)?,
        })
    }
}

// --------- REMOTE Labels -----------------------------------------------------------------------
// NOTE: Local values are update via triggers when remote data changes, since remote always wins.
impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn create_remote_labels<'i>(
        &mut self,
        labels: impl ExactSizeIterator<Item = &'i Label>,
    ) -> DBResult<()> {
        let mut stmt_remote =
                self.0.prepare("INSERT INTO labels_remote (id, parent_id, type, `order`, name, path, color, notified, expanded, sticky) VALUES (?,?,?,?,?,?,?,?,?,?) \
ON CONFLICT (id) DO UPDATE SET `order`=excluded.`order`, name=excluded.name, path=excluded.path, color=excluded.color, parent_id=excluded.parent_id")?;
        for label in labels {
            let sticky: bool = label.sticky.into();
            let expanded: bool = label.expanded.into();
            let notify: bool = label.notify.into();
            stmt_remote.execute((
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
            ))?;
        }
        Ok(())
    }

    pub fn create_remote_label(&mut self, label: &Label) -> DBResult<()> {
        self.create_remote_labels(std::iter::once(label))
    }
    pub fn update_remote_labels<'i>(
        &mut self,
        labels: impl Iterator<Item = &'i Label>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(
            "UPDATE labels_remote SET parent_id=?, `order`=?, name=?, path=?, color=?, \
expanded=?, notified=?, sticky=? WHERE id=?",
        )?;
        for label in labels {
            let sticky: bool = label.sticky.into();
            let expanded: bool = label.expanded.into();
            let notify: bool = label.notify.into();
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
        let mut stmt = self.0.prepare("DELETE FROM labels_remote WHERE id=?")?;
        for id in ids {
            stmt.execute([id])?;
        }
        Ok(())
    }

    pub fn delete_remote_label(&mut self, id: &LabelId) -> DBResult<()> {
        self.delete_remote_labels(std::iter::once(id))
    }

    pub fn get_remote_labels<'i>(
        &self,
        ids: impl ExactSizeIterator<Item = &'i LabelId>,
    ) -> DBResult<Vec<RemoteLabel>> {
        let mut result = Vec::with_capacity(ids.len());
        let query = RemoteLabelSelect::query_in(ids.len());
        let mut stmt = self.0.prepare(&query)?;
        mapped_rows_into_vec(
            &mut result,
            stmt.query_map(params_from_iter(ids), RemoteLabelSelect::from_row)?,
        )?;
        Ok(result)
    }

    pub fn get_remote_label(&self, id: &LabelId) -> DBResult<Option<RemoteLabel>> {
        let query = RemoteLabelSelect::query_with_id();
        self.0
            .query_row(&query, [id], RemoteLabelSelect::from_row)
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

struct RemoteLabelSelect {}

impl RemoteLabelSelect {
    fn query_all() -> &'static str {
        "SELECT id, parent_id, type, `order`, name, path, color, expanded, notified, sticky FROM labels_remote"
    }

    fn query_with_id() -> String {
        format!("{} WHERE id = ?", Self::query_all())
    }
    fn query_in(count: usize) -> String {
        format!(
            "{} WHERE id IN ({})",
            Self::query_all(),
            gen_variable_in_argument_list(count)
        )
    }

    fn from_row(r: &Row) -> DBResult<RemoteLabel> {
        Ok(RemoteLabel {
            id: r.get(0)?,
            label_type: r.get(2)?,
            parent_id: r.get(1)?,
            order: r.get(3)?,
            name: r.get(4)?,
            path: r.get(5)?,
            color: r.get(6)?,
            expanded: r.get(7)?,
            notified: r.get(8)?,
            sticky: r.get(9)?,
        })
    }
}
