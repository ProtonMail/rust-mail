use proton_api_rs::domain::{Event, Label, LabelId, TwoFactorAuth};
use proton_api_rs::exports::anyhow;
use proton_api_rs::exports::log::{error, info, LevelFilter};
use proton_api_rs::http::{Client, ClientBuilder};
use proton_api_rs::LoginError;
use proton_async::async_trait::async_trait;
use proton_async::tokio;
use proton_event_loop::{LoopError, LoopErrorHandlerReply, Subscriber, SubscriberError};
use proton_labels::{Callback, Labels, MemoryStore, ProtonProvider};
use std::pin::pin;
use std::time::Duration;

struct CliCallback {}

impl Callback for CliCallback {
    fn label_created(&mut self, label: &Label) {
        println!("Label Created: {:?}", label)
    }

    fn label_updated(&mut self, label: &Label) {
        println!("Label Updated: {:?}", label)
    }

    fn label_deleted(&mut self, id: &LabelId) {
        println!("Label Deleted: ${id}")
    }
}

struct EventLoopErrorHandler {}
impl proton_event_loop::LoopErrorHandler for EventLoopErrorHandler {
    fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply {
        error!("Event loop error: {error}");
        return LoopErrorHandlerReply::Abort;
    }
}

struct LabelEventSubscriber(tokio::sync::mpsc::Sender<Vec<Event>>);

#[async_trait]
impl Subscriber for LabelEventSubscriber {
    fn name(&self) -> &str {
        "Label Event Subscriber"
    }
    async fn on_events(&mut self, event: &[Event]) -> Result<(), SubscriberError> {
        let event = Vec::from_iter(event.iter().cloned());
        if self.0.is_closed() {
            return Err(SubscriberError::Other(anyhow::anyhow!("channel closed")));
        }
        if let Err(_) = self.0.send(event).await {
            return Err(SubscriberError::Other(anyhow::anyhow!(
                "failed to send on channel"
            )));
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Debug)
        .filter(Some("cookie_store".into()), LevelFilter::Error)
        .filter(Some("rustls".into()), LevelFilter::Error)
        .init();

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        error!("Usage {} email password", args[0]);
        return;
    }

    let email = &args[1];
    let password = &args[2];

    let client = ClientBuilder::new()
        .app_version("Other")
        .connect_timeout(Duration::from_secs(30))
        .build::<Client>()
        .expect("failed to create client");

    let proton_api_rs::SessionType::Authenticated(session) =
        proton_api_rs::Session::login(&client, &email, &password, None)
            .await
            .expect("Failed to login")
    else {
        error!("{}", LoginError::Unsupported2FA(TwoFactorAuth::TOTP));
        return;
    };

    let event_provider = proton_event_loop::ProtonProvider::new(client.clone(), session.clone());
    let event_store = proton_event_loop::InMemoryStore::default();
    let event_error_handler = EventLoopErrorHandler {};

    let event_loop = proton_event_loop::Loop::new();

    proton_event_loop::Loop::new();
    let label_provider = ProtonProvider::new(client.clone(), session.clone());
    let label_store = MemoryStore::new();

    let mut labels = Labels::new(
        Box::new(label_provider),
        Box::new(label_store),
        Box::new(CliCallback {}),
    );

    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);

    info!("Loading labels");
    labels
        .initialize_from_provider()
        .await
        .expect("Failed to init");

    if let Err(e) = event_loop
        .start(
            Duration::from_secs(10),
            Box::new(event_store),
            Box::new(event_provider),
            Box::new(event_error_handler),
        )
        .await
    {
        error!("Failed to start event loop: {e}");
        return;
    }
    let subscriber: Box<dyn Subscriber> = Box::new(LabelEventSubscriber(sender));

    event_loop.subscribe(subscriber).await;
    event_loop.resume();

    {
        let loop_cloned = event_loop.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to wait for ctrl+c");
            loop_cloned.cancel();
        });
    }

    {
        for (idx, label) in labels.get_ordered_labels().into_iter().enumerate() {
            if let Some(path) = label.path {
                println!("[{:02}] {}", idx, path);
            } else {
                println!("[{:02}] {}", idx, label.name);
            }
        }
    }

    println!("Started, waiting on ctrl+c to exit");

    let event_loop = pin!(event_loop);

    loop {
        tokio::select! {
            _ =  event_loop.wait_on_cancelled() => {
                return;
            }

            events = receiver.recv() => {
                let Some(events) = events else {
                    continue;
                };

                for evt in &events {
                    if let Some(events) = &evt.labels {
                        if let Err(e)= labels.on_events(events).await {
                            error!("Failed to apply event ({}): {e}", evt.event_id);
                            return
                        }
                    }
                }
            }
        }
    }
}
