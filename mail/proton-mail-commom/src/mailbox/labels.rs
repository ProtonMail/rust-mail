use crate::{Mailbox, MailboxResult};
use proton_api_mail::domain::LabelType;
use proton_mail_db::LocalLabelWithCount;

impl Mailbox {
    pub fn get_labels_by_type(
        &self,
        label_type: LabelType,
    ) -> MailboxResult<Vec<LocalLabelWithCount>> {
        let v = self.user_ctx.get_labels_by_type(label_type)?;
        Ok(v)
    }
}
