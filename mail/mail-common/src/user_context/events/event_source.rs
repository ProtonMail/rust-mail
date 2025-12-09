use proton_core_common::event_loop::event_source::CoreEventCache;
use proton_event_loop::v6::EventSource;
use proton_mail_api::services::proton::prelude::MailEventV5;

pub struct MailEventSourceV5;

impl EventSource for MailEventSourceV5 {
    type Event = MailEventV5;
    type Cache = CoreEventCache;

    fn name() -> &'static str {
        "mail-event-source"
    }
}
