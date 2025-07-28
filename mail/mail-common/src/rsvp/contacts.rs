use parking_lot::RwLock;
use proton_calendar_common as cal;
use proton_core_api::services::proton::PrivateEmail;
use proton_core_common::{
    UserContext,
    models::{Contact, ContactListWatcher, ModelExtension},
};
use stash::stash::{StashError, Tether};
use std::{collections::HashMap, sync::Arc};
use tracing::{instrument, trace, warn};

#[derive(Clone, Debug, Default)]
pub struct RsvpContacts {
    names: Arc<RwLock<HashMap<PrivateEmail, String>>>,
}

impl RsvpContacts {
    pub async fn new(ctx: &UserContext) -> Result<Self, StashError> {
        let db = ctx.stash();
        let tx = db.connection();
        let this = Self::default();

        this.refresh(&tx).await?;

        ctx.spawn({
            let db = db.clone();
            let this = this.clone();
            let handle = tx.subscribe_to(|sender| Box::new(ContactListWatcher::new(sender)))?;

            async move {
                while handle.next().await.is_ok() {
                    let tx = db.connection();

                    if let Err(err) = this.refresh(&tx).await {
                        warn!("Couldn't refresh RSVP contacts: {err:?}");
                    }
                }
            }
        });

        Ok(this)
    }

    #[instrument(skip_all)]
    async fn refresh(&self, tx: &Tether) -> Result<(), StashError> {
        trace!("Refreshing RSVP contacts");

        let mut names = HashMap::new();

        for mut contact in Contact::all(tx).await? {
            contact.emails(tx).await?;

            for mail in contact.contact_emails {
                names.insert(mail.canonical_email, mail.name);
            }
        }

        trace!("... done, got {} name(s)", names.len());

        *self.names.write() = names;

        Ok(())
    }
}

impl cal::RsvpContacts for RsvpContacts {
    fn lookup_name(&self, email: &str) -> Option<String> {
        self.names.read().get(email).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::MailTestContext;
    use proton_calendar_common::RsvpContacts as _;
    use proton_core_common::models::ContactEmail;
    use std::time::Duration;
    use tokio::time;

    #[tokio::test]
    async fn smoke() {
        let ctx = MailTestContext::new().await;
        let ctx = ctx.uninitialized_mail_user_context().await;

        // ---

        let mut bj = Contact {
            remote_id: Some("100".into()),
            contact_emails: vec![ContactEmail {
                remote_id: Some("1000".into()),
                canonical_email: "bj@pm.me".into(),
                name: "Bonnie Jovie".into(),
                ..ContactEmail::test_default()
            }],
            ..Contact::test_default()
        };

        let mut rs = Contact {
            remote_id: Some("101".into()),
            contact_emails: vec![ContactEmail {
                remote_id: Some("1001".into()),
                canonical_email: "rs@pm.me".into(),
                name: "Ringi Starri".into(),
                ..ContactEmail::test_default()
            }],
            ..Contact::test_default()
        };

        let mut db = ctx.user_stash().connection();

        for contact in [&mut bj, &mut rs] {
            db.tx(async |tx| contact.save(tx).await).await.unwrap();

            for email in &mut contact.contact_emails {
                db.tx(async |tx| email.save(tx).await).await.unwrap();
            }
        }

        // ---

        let target = RsvpContacts::new(ctx.user_context()).await.unwrap();

        assert_eq!(Some("Bonnie Jovie".into()), target.lookup_name("bj@pm.me"));
        assert_eq!(Some("Ringi Starri".into()), target.lookup_name("rs@pm.me"));
        assert_eq!(None, target.lookup_name("kf@pm.me"));

        // ---

        rs.contact_emails[0].name = "Ringo Starro".into();

        let mut kf = Contact {
            remote_id: Some("102".into()),
            contact_emails: vec![ContactEmail {
                remote_id: Some("1002".into()),
                canonical_email: "kf@pm.me".into(),
                name: "King Fisher".into(),
                ..ContactEmail::test_default()
            }],
            ..Contact::test_default()
        };

        for contact in [&mut bj, &mut rs, &mut kf] {
            db.tx(async |tx| contact.save(tx).await).await.unwrap();

            for email in &mut contact.contact_emails {
                db.tx(async |tx| email.save(tx).await).await.unwrap();
            }
        }

        // ---

        // Update happens in a background task to which we don't have a direct
        // signal - so let's wait until the task notices the database has
        // changed
        let assert = async {
            loop {
                if target.lookup_name("bj@pm.me") == Some("Bonnie Jovie".into())
                    && target.lookup_name("rs@pm.me") == Some("Ringo Starro".into())
                    && target.lookup_name("kf@pm.me") == Some("King Fisher".into())
                {
                    break;
                }

                time::sleep(Duration::from_millis(1)).await;
            }
        };

        time::timeout(Duration::from_secs(5), assert).await.unwrap();
    }
}
