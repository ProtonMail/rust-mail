use std::sync::Arc;

use proton_api_core::services::proton::common::LabelId;
use proton_core_common::{
    datatypes::SystemLabel,
    models::{Label, ModelIdExtension},
};
use stash::orm::Model;
use tokio::task::yield_now;

use crate::{
    datatypes::{ReadFilter, ViewMode},
    mail_scroller::MailScroller,
    models::{Conversation, MailSettings, Message},
    MailContextError, MailUserContext,
};

pub struct Prefetch {
    ctx: Arc<MailUserContext>,
    prefetch_count: usize,
    prefetch_locations: Vec<Location>,
}

struct Location {
    label_id: LabelId,
    location: LocationKind,
}

enum LocationKind {
    Conversations,
    Messages,
}

impl Prefetch {
    pub async fn key_locations(ctx: Arc<MailUserContext>) {
        let tether = ctx.user_stash().connection();
        let Ok(Some(mail_settings)) = MailSettings::get(&tether).await else {
            tracing::error!("Failed to get mail settings");
            return;
        };

        let inbox_location = match mail_settings.view_mode {
            ViewMode::Conversations => LocationKind::Conversations,
            ViewMode::Messages => LocationKind::Messages,
        };
        let prefetch_count = 10;
        let locations = vec![
            Location {
                label_id: SystemLabel::Inbox.remote_id(),
                location: inbox_location,
            },
            Location {
                label_id: SystemLabel::Sent.remote_id(),
                location: LocationKind::Messages,
            },
            Location {
                label_id: SystemLabel::AllSent.remote_id(),
                location: LocationKind::Messages,
            },
            Location {
                label_id: SystemLabel::Drafts.remote_id(),
                location: LocationKind::Messages,
            },
            Location {
                label_id: SystemLabel::AllDrafts.remote_id(),
                location: LocationKind::Messages,
            },
        ];

        let this = Self {
            ctx,
            prefetch_count,
            prefetch_locations: locations,
        };

        tokio::spawn(async move {
            let _ = this.prefetch().await;
        });
    }

    async fn prefetch(self) -> Result<(), MailContextError> {
        let mut tether = self.ctx.user_stash().connection();

        for Location { label_id, location } in &self.prefetch_locations {
            yield_now().await;
            match location {
                LocationKind::Conversations => {
                    let Some(local_label_id) =
                        Label::remote_id_counterpart(label_id.clone(), &tether).await?
                    else {
                        continue;
                    };
                    let Ok(mut scroller) = MailScroller::conversations(
                        self.ctx.clone(),
                        local_label_id,
                        ReadFilter::All,
                        50,
                    )
                    .await
                    else {
                        continue;
                    };
                    yield_now().await;

                    let items = scroller.fetch_more().await?;

                    yield_now().await;
                    for item in items.into_iter().take(self.prefetch_count) {
                        let api = self.ctx.api();
                        let _ = Conversation::sync_conversation_messages(
                            item.local_id,
                            &mut tether,
                            api,
                        )
                        .await;
                        yield_now().await;
                        let messages = Message::in_conversation(item.local_id, &tether).await?;
                        yield_now().await;
                        let Some(label) = Label::load(local_label_id, &tether).await? else {
                            continue;
                        };
                        let Ok(message_id_to_open) =
                            Conversation::message_id_to_open(item.local_id, &label, &messages)
                        else {
                            continue;
                        };
                        yield_now().await;

                        let _ = Message::message_body(self.ctx.clone(), message_id_to_open).await;
                        yield_now().await;
                    }
                }
                LocationKind::Messages => {
                    let Some(local_label_id) =
                        Label::remote_id_counterpart(label_id.clone(), &tether).await?
                    else {
                        continue;
                    };
                    let Ok(mut scroller) = MailScroller::messages(
                        self.ctx.clone(),
                        local_label_id,
                        ReadFilter::All,
                        50,
                    )
                    .await
                    else {
                        continue;
                    };
                    yield_now().await;
                    let items = scroller.fetch_more().await?;
                    yield_now().await;
                    for item in items.into_iter().take(self.prefetch_count) {
                        let _ =
                            Message::message_body(self.ctx.clone(), item.local_id.unwrap()).await;
                        yield_now().await;
                    }
                }
            }
        }

        Ok(())
    }
}
