use crate::provider::Provider;
use crate::store::Store;
use crate::subscriber::{Subscriber, SubscriberError};
use proton_api_rs::domain::{Event, EventId, MoreEvents};
use proton_api_rs::exports::anyhow;
use proton_api_rs::exports::log::debug;
use proton_api_rs::exports::thiserror;
use proton_api_rs::http;
use proton_api_rs::http::Error;
use proton_async::tokio;
use proton_async::tokio_util::sync::CancellationToken;
use std::time::Duration;

pub struct Loop {
    store: Box<dyn Store>,
    provider: Box<dyn Provider>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    #[error("Failed to read from store: {0}")]
    StoreRead(anyhow::Error),
    #[error("Failed to write store: {0}")]
    StoreWrite(anyhow::Error),
    #[error("Failed to retrieve event: {0}")]
    Provider(#[from] Error),
    #[error("Subscriber failed to apply event: {0}")]
    Subscriber(anyhow::Error),
}

const MAX_EVENTS_PER_POLL: usize = 50;

//TODO(@Leander): Unsubscribe
//TODO(@Leander): Error reporting and recovery
//TODO(@Leander): Pause Resume
//TODO(@Leander): Self contained task execution since we no longer rely on wasm async
impl Loop {
    pub fn new(store: Box<dyn Store>, provider: Box<dyn Provider>) -> Self {
        Self { store, provider }
    }

    pub async fn run(
        &mut self,
        token: CancellationToken,
        poll_interval: Duration,
        subscribers: impl IntoIterator<Item = Box<dyn Subscriber>>,
    ) -> Result<(), LoopError> {
        let mut last_event_id = match self.store.load().await.map_err(LoopError::StoreRead)? {
            Some(id) => id,
            None => {
                debug!("No event id in event store, retrieving latest");
                let id = self.provider.get_latest_event_id().await?;
                self.store.store(&id).await.map_err(LoopError::StoreRead)?;
                id
            }
        };

        let subscribers = subscribers.into_iter().collect::<Vec<_>>();

        let mut events = Vec::with_capacity(MAX_EVENTS_PER_POLL);

        let interval = tokio::time::interval(poll_interval);
        let mut interval = std::pin::pin!(interval);

        debug!("Starting loop");
        loop {
            tokio::select! {
                _= token.cancelled() => {
                    return Ok(());
                }

                _= interval.tick() => {
                    self.collect_events(&last_event_id, &mut events).await?;

                    if events
                        .last()
                        .expect("should be at least one event object present")
                        .event_id
                        == last_event_id
                    {
                        debug!("No new events");
                        //no new api events
                        continue;
                    }

                    debug!("Received new events: {:?}", events.iter().map(|e| e.event_id.clone()).collect::<Vec<_>>());

                    for subscriber in &subscribers {
                        if let Err(e) = subscriber.on_events(&events).await {
                            match e {
                                SubscriberError::Http(e) => {
                                    match e {
                                        Error::Redirect(_, _)
                                        | Error::Timeout(_)
                                        | Error::Connection(_) => {
                                            // failed due to network error try again later
                                            continue;
                                        }
                                        _ => return Err(LoopError::Subscriber(anyhow::anyhow!(e))),
                                    }
                                }
                                SubscriberError::Other(e) => return Err(LoopError::Subscriber(e)),
                            }
                        }
                    }

                    let new_event_id = events
                        .last()
                        .expect("should be at least one event object present")
                        .event_id
                        .clone();
                    if let Err(e) = self.store.store(&new_event_id).await {
                        return Err(LoopError::StoreWrite(e));
                    }

                    last_event_id = new_event_id;
                }
            }
        }
    }

    async fn collect_events(
        &self,
        last_event_id: &EventId,
        out: &mut Vec<Event>,
    ) -> http::Result<()> {
        out.clear();

        let event = self.provider.get_event(last_event_id).await?;

        let mut has_more = event.more == MoreEvents::Yes;
        let mut next_event_id = event.event_id.clone();
        out.push(event);

        let mut num_collected = 0_usize;

        while has_more {
            num_collected += 1;
            if num_collected >= MAX_EVENTS_PER_POLL {
                return Ok(());
            }

            let event = self.provider.get_event(&next_event_id).await?;
            has_more = event.more == MoreEvents::Yes;
            next_event_id = event.event_id.clone();
            out.push(event);
        }

        Ok(())
    }
}
