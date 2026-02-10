use proton_calendar_common as cal;
use proton_core_common::models::ContactEmail;
use stash::{UserDb, orm::Model, stash::Stash};
use tracing::warn;

#[derive(Debug)]
pub struct RsvpContacts {
    stash: Stash<UserDb>,
}

impl RsvpContacts {
    pub fn new(stash: &Stash<UserDb>) -> Self {
        Self {
            stash: stash.clone(),
        }
    }
}

impl cal::RsvpContacts for RsvpContacts {
    async fn get_display_name(&self, email: &str) -> Option<String> {
        let contact = async {
            let tether = self.stash.connection().await?;
            ContactEmail::find_first(
                "WHERE canonical_email = ?",
                vec![Box::new(email.to_string())],
                &tether,
            )
            .await
        }
        .await;

        match contact {
            Ok(Some(contact)) => Some(contact.name),
            Ok(None) => None,

            Err(err) => {
                warn!("Couldn't get display name for contact: {err:?}");

                // Not the end of the world, names are sorta "advisory" anyway -
                // we don't need them for RSVP logic, we pull them just for UX
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::MailTestContext;
    use proton_calendar_common::RsvpContacts as _;
    use proton_core_common::models::{Contact, ContactEmail};

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

        let mut stash = ctx.user_stash().connection().await.unwrap();

        for contact in [&mut bj, &mut rs] {
            stash
                .tx(async |tether| contact.save(tether).await)
                .await
                .unwrap();

            for email in &mut contact.contact_emails {
                stash
                    .tx(async |tether| email.save(tether).await)
                    .await
                    .unwrap();
            }
        }

        // ---

        let target = RsvpContacts::new(ctx.user_stash());

        assert_eq!(
            Some("Bonnie Jovie".into()),
            target.get_display_name("bj@pm.me").await
        );
        assert_eq!(
            Some("Ringi Starri".into()),
            target.get_display_name("rs@pm.me").await
        );
        assert_eq!(None, target.get_display_name("kf@pm.me").await);
    }
}
