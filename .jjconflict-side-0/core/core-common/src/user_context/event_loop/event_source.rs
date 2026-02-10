use crate::event_loop::v6::CoreEventCache;
use proton_core_api::services::proton::CoreEvent;
use proton_event_loop::v6::EventSource;

pub struct CoreEventSource;

impl EventSource for CoreEventSource {
    type Event = CoreEvent;
    type Cache = CoreEventCache;

    fn name() -> &'static str {
        "core"
    }
}
