use proton_api_mail::domain::{Label, LabelId, LabelType, MailEvent};
use proton_api_mail::proton_api_core::domain::TwoFactorAuth;
use proton_api_mail::proton_api_core::exports::anyhow;
use proton_api_mail::proton_api_core::exports::tracing::{error, info};
use proton_api_mail::proton_api_core::http::ClientBuilder;
use proton_api_mail::proton_api_core::{LoginError, Session};
use proton_api_mail::{proton_api_core, MailSession};
use proton_event_loop::{
    ChannelledSubscriber, EventLoopError, EventLoopErrorHandlerReply, Subscriber, SubscriberError,
};
use proton_mail_labels::{
    Callback, LabelView, Labels, MemoryStore, ProtonProvider, UILabelViewCallback,
};
use std::pin::pin;
use std::time::Duration;

struct CliCallback {}

impl Callback for CliCallback {
    fn label_created(&self, label: &Label) {
        println!("Label Created: {:?}", label)
    }

    fn label_updated(&self, label: &Label) {
        println!("Label Updated: {:?}", label)
    }

    fn label_deleted(&self, id: &LabelId) {
        println!("Label Deleted: ${id}")
    }
}

struct EventLoopErrorHandler {}
impl proton_event_loop::EventLoopErrorHandler for EventLoopErrorHandler {
    fn on_error(&self, error: EventLoopError) -> EventLoopErrorHandlerReply {
        error!("Event loop error: {error}");
        return EventLoopErrorHandlerReply::Abort;
    }
}

struct UICallback {}

impl UILabelViewCallback for UICallback {
    fn on_pending(&self) {}
}

fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    proton_api_core::exports::tracing::subscriber::set_global_default(subscriber)
        .expect("failed to register tracing subscriber");

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
        .build()
        .expect("failed to create client");

    let proton_api_mail::proton_api_core::SessionType::Authenticated(session) =
        proton_mail_labels::static_runtime()
            .block_on(async { Session::login(client, &email, &password, None).await })
            .expect("Failed to login")
    else {
        error!("{}", LoginError::Unsupported2FA(TwoFactorAuth::TOTP));
        return;
    };

    let event_provider = proton_event_loop::ProtonProvider::new(session.clone());
    let event_store = proton_event_loop::InMemoryStore::default();
    let event_error_handler = EventLoopErrorHandler {};

    let event_loop = proton_event_loop::BackgroundEventLoop::new();

    proton_event_loop::BackgroundEventLoop::<MailEvent>::new();
    let label_provider = ProtonProvider::new(MailSession::new(session.clone()));
    let label_store = MemoryStore::new();

    let mut labels = Labels::new(Box::new(label_provider), Box::new(label_store));

    labels.add_callback(Box::new(CliCallback {}));

    let (sender, mut receiver) = ChannelledSubscriber::new("labels".into());

    info!("Loading labels");
    labels.initialize_from_provider().expect("Failed to init");

    if let Err(e) = proton_mail_labels::static_runtime().block_on(async {
        event_loop
            .start(
                Duration::from_secs(10),
                Box::new(event_store),
                Box::new(event_provider),
                Box::new(event_error_handler),
            )
            .await
    }) {
        error!("Failed to start event loop: {e}");
        return;
    }
    let subscriber: Box<dyn Subscriber<MailEvent>> = Box::new(sender);

    proton_mail_labels::static_runtime().block_on(async {
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
    });

    let mut label_view = LabelView::new(&mut labels, LabelType::Label, Box::new(UICallback {}))
        .expect("failed to crete view");

    {
        for (idx, label) in label_view.as_ref().into_iter().enumerate() {
            if let Some(path) = &label.path {
                println!("[{:02}] {}", idx, path);
            } else {
                println!("[{:02}] {}", idx, label.name);
            }
        }
    }

    println!("Started, waiting on ctrl+c to exit");

    proton_mail_labels::static_runtime().block_on(async {

    let event_loop = pin!(event_loop);

    loop {
        tokio::select! {
            _ =  event_loop.wait_on_cancelled() => {
                return;
            }

            _ = receiver.handle_events_async(|events| -> Result<(), SubscriberError> {
                    for evt in events {
                        if let Some(events) = &evt.labels {
                            if let Err(e)= labels.on_events(events) {
                                error!("Failed to apply event ({}): {e}", evt.event_id);
                                return Err(SubscriberError::Other(anyhow::anyhow!("Failed to apply event ({}): {e}", evt.event_id)));
                            }
                        }
                    }

                if label_view.has_pending_changes() {
                    info!("Label view has pending changes");
                    label_view.consume_pending_changes();
                }
                Ok(())
            })=> {
            }
        }
    }
    })
}
