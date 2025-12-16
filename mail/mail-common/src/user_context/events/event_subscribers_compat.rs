//! Compatability wrappers to run v5 event loop code on top of v6 setup.

use crate::events::event_source::MailEventSourceV5;
use proton_core_common::event_loop::event_source::CoreEventSource;
use proton_event_loop::v6::{EventSource, EventSubscriber};
use proton_event_loop::{EventSubscriberResult, RefreshFlag};

pub struct MailEventV5SubscriberCompat<T: EventSubscriber<CoreEventSource>>(pub T);
#[async_trait::async_trait]
impl<T> EventSubscriber<MailEventSourceV5> for MailEventV5SubscriberCompat<T>
where
    T: EventSubscriber<CoreEventSource>,
{
    fn name(&self) -> &'static str {
        self.0.name()
    }

    async fn on_event(
        &self,
        event: &<MailEventSourceV5 as EventSource>::Event,
        cache: &mut <MailEventSourceV5 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        self.0.on_event(&event.core, cache).await
    }

    async fn on_refresh(
        &self,
        refresh_flag: RefreshFlag,
        cache: &mut <MailEventSourceV5 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        self.0.on_refresh(refresh_flag, cache).await
    }
}
