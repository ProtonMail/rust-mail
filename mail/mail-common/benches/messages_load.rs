mod profiler;
use criterion::{Criterion, criterion_group, criterion_main};
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_common::datatypes::{AddressStatus, AddressType, LabelType};
use proton_core_common::db::migrations::migrate_core_db;
use proton_core_common::models::{Address, Label, ModelExtension};
use proton_mail_api::services::proton::common::{AttachmentId, ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::{AttachmentMetadata, Disposition, MessageFlags};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::db::migrations::migrate_db;
use proton_mail_common::models::Message;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashConfiguration, StashError};
use std::string::ToString;
use tempdir::TempDir;
use uuid::Uuid;

pub fn current_benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    c.bench_function("save_state", |b| {
        let temp_dir = TempDir::new("message-bench").unwrap();
        let db_path = temp_dir.path().join("sqlite3.db");
        let stash_config = StashConfiguration {
            path: Some(&db_path),
            pool_size: None,
            idle_count: None,
        };
        let stash = Stash::new(stash_config).unwrap();
        let (label_id, address_id) = runtime.block_on(async { setup_db(&stash).await });
        b.iter(|| {
            let mut tether = stash.connection();
            runtime.block_on(async {
                tether
                    .tx::<_, _, StashError>(async |tx| {
                        create_messages(tx, label_id.clone(), address_id.clone(), 100).await;
                        Ok(())
                    })
                    .await
                    .unwrap();
            });
        })
    });

    c.bench_function("load_messages", |b| {
        let temp_dir = TempDir::new("message-bench").unwrap();
        let db_path = temp_dir.path().join("sqlite3.db");
        let stash_config = StashConfiguration {
            path: Some(&db_path),
            pool_size: None,
            idle_count: None,
        };
        let stash = Stash::new(stash_config).unwrap();
        runtime.block_on(async { setup_and_create_messages(&stash, 100).await });
        b.iter(|| {
            let tether = stash.connection();
            runtime.block_on(async {
                let _ = Message::all(&tether).await.unwrap();
            })
        })
    });
    c.bench_function("load_messages_with_tx", |b| {
        let temp_dir = TempDir::new("message-bench").unwrap();
        let db_path = temp_dir.path().join("sqlite3.db");
        let stash_config = StashConfiguration {
            path: Some(&db_path),
            pool_size: None,
            idle_count: None,
        };
        let stash = Stash::new(stash_config).unwrap();
        runtime.block_on(async { setup_and_create_messages(&stash, 100).await });
        b.iter(|| {
            let tether = stash.connection();
            runtime.block_on(async {
                tether.execute("BEGIN", vec![]).await.unwrap();
                let _ = Message::all(&tether).await.unwrap();
                tether.execute("END", vec![]).await.unwrap();
            })
        })
    });
}

async fn setup_db(stash: &Stash) -> (LabelId, AddressId) {
    migrate_core_db(stash).await.unwrap();
    migrate_db(stash).await.unwrap();
    let address_id: AddressId = AddressId::from(Uuid::new_v4().to_string());
    let label_id: LabelId = LabelId::from(Uuid::new_v4().to_string());

    let mut tether = stash.connection();

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut address = Address {
                local_id: None,
                remote_id: Some(address_id.clone()),
                address_type: AddressType::Original,
                catch_all: false,
                display_name: "".to_string(),
                display_order: 0,
                domain_id: None,
                email: "".to_string(),
                keys: Default::default(),
                proton_mx: false,
                receive: false,
                send: false,
                signature: "".to_string(),
                signed_key_list: Default::default(),
                status: AddressStatus::Enabled,
            };
            address.save(tx).await.unwrap();

            let mut label = Label {
                local_id: None,
                remote_id: Some(label_id.clone()),
                local_parent_id: None,
                remote_parent_id: None,
                color: Default::default(),
                display: false,
                expanded: false,
                label_type: LabelType::Folder,
                name: "".to_string(),
                notify: false,
                display_order: 0,
                path: None,
                sticky: false,
            };
            label.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    (label_id, address_id)
}

async fn create_messages(
    tx: &Bond<'_>,
    label_id: LabelId,
    address_id: AddressId,
    message_count: usize,
) {
    Message::delete_all(tx).await.unwrap();
    for order in 0..message_count {
        new_message_random_message(tx, address_id.clone(), label_id.clone(), order + 1).await;
    }
}

async fn setup_and_create_messages(stash: &Stash, message_count: usize) {
    let (label_id, address_id) = setup_db(stash).await;
    let mut tether = stash.connection();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for order in 0..message_count {
                new_message_random_message(tx, address_id.clone(), label_id.clone(), order + 1)
                    .await;
            }
            Ok(())
        })
        .await
        .unwrap();
}

async fn new_message_random_message(
    tx: &Bond<'_>,
    address_id: AddressId,
    label_id: LabelId,
    order: usize,
) -> Message {
    let message = proton_mail_api::services::proton::response_data::MessageMetadata {
        id: MessageId::from(Uuid::new_v4().to_string()),
        conversation_id: ConversationId::from(Uuid::new_v4().to_string()),
        address_id,
        attachments_metadata: vec![
            AttachmentMetadata {
                id: AttachmentId::from(Uuid::new_v4().to_string()),
                disposition: Disposition::Attachment,
                mime_type: "text/plain".to_string(),
                name: Uuid::new_v4().to_string(),
                size: 1024,
            },
            AttachmentMetadata {
                id: AttachmentId::from(Uuid::new_v4().to_string()),
                disposition: Disposition::Inline,
                mime_type: "image/png".to_string(),
                name: Uuid::new_v4().to_string(),
                size: 1024,
            },
        ],
        bcc_list: vec![],
        cc_list: vec![],
        expiration_time: 0,
        external_id: None,
        flags: MessageFlags::RECEIVED,
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![LabelId::all_mail(), LabelId::inbox(), label_id],
        num_attachments: 1,
        order: order as u64,
        sender: Default::default(),
        size: 1204,
        snooze_time: 0,
        subject: format!("Message Order={order}"),
        time: (order + 100) as u64,
        to_list: vec![],
        unread: false,
    };

    let mut message = Message::from_api_metadata(message, tx).await.unwrap();
    message.save(tx).await.unwrap();
    message
}
fn profiled() -> Criterion {
    Criterion::default().with_profiler(profiler::FlamegraphProfiler::new(100))
}

criterion_group!(
    name = benches;
    config = profiled();
    targets = current_benchmark
);
criterion_main!(benches);
