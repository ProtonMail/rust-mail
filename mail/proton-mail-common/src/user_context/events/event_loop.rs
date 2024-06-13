use crate::actions::EventLoopAction;
use crate::user_context::events::subscriber::MailEventSubscriber;
use crate::{MailContextResult, MailUserContext, WeakMailUserContext};
use async_trait::async_trait;
use proton_api_mail::proton_api_core;
use proton_api_mail::proton_api_core::domain::{ContactEmailEvent, ContactEvent, Event, EventId, ProductUsedSpace, User, UserSettings};
use proton_api_mail::proton_api_core::exports::anyhow;
use proton_api_mail::proton_api_core::exports::anyhow::anyhow;
use proton_api_mail::proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_mail::proton_api_core::exports::tracing::error;
use proton_core_common::CoreEventSubscriber;
use proton_event_loop::EventLoopError;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde")]
pub struct MailEvent {
    #[serde(flatten)]
    pub(super) event: proton_api_mail::domain::MailEvent,
}

impl Event for MailEvent {
    fn event_id(&self) -> &EventId {
        self.event.event_id()
    }

    fn has_more(&self) -> bool {
        self.event.has_more()
    }
}

impl proton_core_common::CoreEvent for MailEvent {
    fn get_core_event_user(&self) -> Option<&User> {
        self.event.user.as_ref()
    }

    fn get_core_event_user_settings(&self) -> Option<&UserSettings> {
        self.event.user_settings.as_ref()
    }

    fn get_core_event_used_space(&self) -> Option<i64> {
        self.event.used_space
    }

    fn get_core_event_used_product_space(&self) -> Option<&ProductUsedSpace> {
        self.event.product_used_space.as_ref()
    }

    fn get_core_event_addresses(&self) -> Option<&[proton_api_core::domain::Address]> {
        self.event.addresses.as_deref()
    }
    
    fn get_core_event_contacts(&self) -> Option<&[ContactEvent]> {
        unimplemented!()
    }

    fn get_core_event_contact_emails(&self) -> Option<&[ContactEmailEvent]> {
        unimplemented!()
    }
}

const MAIL_EVENT_TYPE_ID: &str = "proton-mail-event";

impl proton_event_loop::Store for MailUserContext {
    fn load(&self) -> anyhow::Result<Option<EventId>> {
        let conn = self.new_db_connection().map_err(|e| {
            error!("Failed to acquire db connection: {e}");
            anyhow!("Failed to acquire db connection")
        })?;
        conn.read(|conn| {
            conn.get_last_event_id(MAIL_EVENT_TYPE_ID).map_err(|e| {
                error!("Failed to load event id from db:{e}");
                anyhow!("Failed to load event id {e}")
            })
        })
    }

    fn store(&self, id: &EventId) -> anyhow::Result<()> {
        let mut conn = self.new_db_connection().map_err(|e| {
            error!("Failed to acquire db connection: {e}");
            anyhow!("Failed to acquire db connection")
        })?;
        conn.tx(|tx| tx.set_last_event_id(MAIL_EVENT_TYPE_ID, id))
            .map_err(|e| {
                error!("Failed to store event id in db:{e}");
                anyhow!("Failed to store event id {e}")
            })
    }
}

#[async_trait]
impl proton_event_loop::Provider<MailEvent> for MailUserContext {
    async fn get_latest_event_id(&self) -> proton_api_core::http::Result<EventId> {
        self.session().get_latest_event().await
    }

    async fn get_event(&self, event_id: &EventId) -> proton_api_core::http::Result<MailEvent> {
        self.session()
            .get_event_with_conv_and_msg_counts::<MailEvent>(event_id)
            .await
    }
}

impl MailUserContext {
    pub fn queue_event_loop_poll(&self) -> MailContextResult<()> {
        self.queue_action(EventLoopAction {})
    }

    pub async fn poll_event_loop(&self) -> Result<(), EventLoopError> {
        let weak_ctx = WeakMailUserContext::new(self);
        let core_subscriber = CoreEventSubscriber::new(weak_ctx.clone());
        let mail_subscriber = MailEventSubscriber::new(weak_ctx);
        //TODO: better way to store this.
        let subscribers: [Box<dyn proton_event_loop::Subscriber<MailEvent>>; 2] =
            [Box::new(core_subscriber), Box::new(mail_subscriber)];
        self.inner.event_loop.poll(self, self, &subscribers).await
    }
}
