use proton_api_core::services::proton::common::LabelId;
use stash::params;
use stash::stash::{Bond, StashError};

use crate::datatypes::SystemLabelId;

pub(crate) fn default_labels() -> [(LabelId, &'static str); 19] {
    [
        (LabelId::inbox(), "Inbox"),
        (LabelId::starred(), "Starred"),
        (LabelId::drafts(), "Drafts"),
        (LabelId::sent(), "Sent"),
        (LabelId::archive(), "Archive"),
        (LabelId::spam(), "Spam"),
        (LabelId::trash(), "Trash"),
        (LabelId::all_mail(), "All Mail"),
        (LabelId::almost_all_mail(), "Almost All Mail"),
        (LabelId::outbox(), "Outbox"),
        (LabelId::all_drafts(), "All Drafts"),
        (LabelId::all_sent(), "All Sent"),
        (LabelId::all_scheduled(), "All Scheduled"),
        (LabelId::snoozed(), "Snoozed"),
        (LabelId::category_social(), "Category Social"),
        (LabelId::category_promotions(), "Category Promotions"),
        (LabelId::category_updates(), "Category Updates"),
        (LabelId::category_forums(), "Category Forums"),
        (LabelId::category_default(), "Category Default"),
    ]
}

pub async fn create_default_labels(tx: &Bond<'_>) -> Result<(), StashError> {
    // Insert default known system
    let sql = r#"INSERT INTO labels (remote_id, label_type, name, color, display_order) VALUES (?,4,?,'#000000',?)"#;

    for (index, (id, name)) in default_labels().into_iter().enumerate() {
        tx.execute(sql, params![id, name, index]).await?;
    }

    Ok(())
}
