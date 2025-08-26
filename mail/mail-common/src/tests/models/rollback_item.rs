use crate as proton_mail_common;
use crate::datatypes::LocalConversationId;
use proton_core_api::services::proton::GetLabelsByIdsOptions;
use proton_core_api::services::proton::GetLabelsResponse;
use proton_core_api::session::{Config, EnvId, Session};
use proton_core_common::models::ModelExtension;
use proton_core_common::models::ModelIdExtension;
use proton_core_common::test_utils::test_context::MockApiEnv;
use proton_core_common::test_utils::utils::mock_auth_endpoints;
use proton_mail_api::services::proton::common::ConversationId;
use proton_mail_api::services::proton::responses::{GetConversationsResponse, GetMessagesResponse};
use proton_mail_common::test_utils::db::new_test_connection_file;
use proton_mail_common::{
    api_conversation, api_label, api_message_meta, conversation, label, message,
    test_utils::utils::create_address,
};
use test_case::test_case;
#[allow(unused_imports)]
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path, query_param_contains},
};

use super::*;

#[test_case(vec![
    conversation!(remote_id: Some("123".into())).into(),
    message!(remote_id: Some("123".into())).into()
], None; "Test 1: 2 different items with the same remote_id")]
#[test_case(vec![
    label!(remote_id: Some("123".into())).into(),
], None; "Test 2: Only one label")]
#[test_case(vec![
    label!(remote_id: Some("123".into())).into(),
    label!(remote_id: Some("124".into())).into(),
    label!(remote_id: Some("125".into())).into(),
    label!(remote_id: Some("126".into())).into(),
    label!(remote_id: Some("127".into())).into(),
    label!(remote_id: Some("128".into())).into(),
], None; "Test 3: 6 labels")]
#[test_case(vec![
    conversation!(remote_id: Some("123".into())).into(),
    message!(remote_id: Some("123".into())).into(),
    message!(remote_id: Some("124".into())).into(),
    message!(remote_id: Some("125".into())).into(),
    message!(remote_id: Some("126".into())).into(),
    message!(remote_id: Some("127".into())).into(),
    message!(remote_id: Some("128".into())).into(),
], None; "Test 4: 6 messages")]
#[test_case(vec![
    label!(remote_id: Some("123".into())).into(),
    label!(remote_id: Some("124".into())).into(),
    label!(remote_id: Some("125".into())).into(),
    label!(remote_id: Some("126".into())).into(),
    label!(remote_id: Some("127".into())).into(),
    label!(remote_id: Some("128".into())).into(),
    message!(remote_id: Some("123".into())).into(),
    message!(remote_id: Some("124".into())).into(),
    message!(remote_id: Some("125".into())).into(),
    message!(remote_id: Some("126".into())).into(),
    message!(remote_id: Some("127".into())).into(),
    message!(remote_id: Some("128".into())).into(),
    conversation!(remote_id: Some("123".into())).into(),
], None; "Test 5: 13 different items")]
#[test_case(vec![
    conversation!(remote_id: Some("123".into())).into(),
    conversation!(remote_id: Some("124".into())).into(),
    conversation!(remote_id: Some("125".into())).into(),
    conversation!(remote_id: Some("126".into())).into(),
    conversation!(remote_id: Some("127".into())).into(),
    conversation!(remote_id: Some("128".into())).into(),
], None; "Test 6: 6 conversations")]
#[test_case(vec![
    label!(remote_id: Some("123".into())).into(),
    label!(remote_id: Some("124".into())).into(),
    label!(remote_id: Some("125".into())).into(),
    label!(remote_id: Some("126".into())).into(),
    label!(remote_id: Some("127".into())).into(),
    label!(remote_id: Some("128".into())).into(),
    message!(remote_id: Some("123".into())).into(),
    message!(remote_id: Some("124".into())).into(),
    message!(remote_id: Some("125".into())).into(),
    message!(remote_id: Some("126".into())).into(),
    message!(remote_id: Some("127".into())).into(),
    message!(remote_id: Some("128".into())).into(),
    conversation!(remote_id: Some("123".into())).into(),
    conversation!(remote_id: Some("124".into())).into(),
    conversation!(remote_id: Some("125".into())).into(),
    conversation!(remote_id: Some("126".into())).into(),
    conversation!(remote_id: Some("127".into())).into(),
    conversation!(remote_id: Some("128".into())).into(),
], None; "Test 7: 18 different items")]
#[test_case(vec![
    conversation!(remote_id: Some("123".into())).into(),
    conversation!(remote_id: Some("123".into())).into(),
    conversation!(remote_id: Some("123".into())).into(),
    conversation!(remote_id: Some("123".into())).into(),
    conversation!(remote_id: Some("123".into())).into(),
    conversation!(remote_id: Some("123".into())).into(),
], Some(vec![conversation!(remote_id: Some("123".into())).into()]); "Test 8: 6 exactly the same conversations")]
#[tokio::test]
async fn test_store_and_delete_remote_items(
    mut input: Vec<RollbackItem>,
    mut expected: Option<Vec<RollbackItem>>,
) {
    // * RollbackItem is correctly stored *
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection();
    tether
        .tx::<_, _, StashError>(async |tx| {
            if let Some(items) = &mut expected {
                for item in items {
                    item.save(tx).await.unwrap();
                }
            }

            for item in &mut input {
                item.save(tx).await.unwrap();
            }

            Ok(())
        })
        .await
        .unwrap();

    let expected = expected.unwrap_or(input);
    let actual = RollbackItem::all(&tether).await.unwrap();

    assert_eq!(expected, actual);

    // * RollbackItem is correctly synced *
    setup_database(&mut tether).await;

    const BATCH_SIZE: usize = 2;

    let (_mock, api) = start_server(&tether, BATCH_SIZE).await;

    let mut tether = stash.connection();
    RollbackItem::sync_all(&api, &mut tether, BATCH_SIZE)
        .await
        .unwrap();

    // * RollbackItems are correctly deleted during sync *
    let actual = RollbackItem::all(&tether).await.unwrap();

    assert_eq!(actual.len(), 0);

    // * RollbackItems with no limit for empty stash *
    RollbackItem::sync_all(&api, &mut tether, None)
        .await
        .unwrap();
}

async fn setup_database(tether: &mut Tether) {
    let mut conversations = conversations(tether).await;
    let mut local_conversation_id = None;
    let mut remote_conversation_id = None;

    tether
        .tx::<_, _, StashError>(async |tx| {
            for conversation in conversations.iter_mut() {
                conversation.save(tx).await.unwrap();
                local_conversation_id = conversation.local_id;
                remote_conversation_id = conversation.remote_id.clone();
            }
            Ok(())
        })
        .await
        .unwrap();

    let mut labels = labels(tether).await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            for label in labels.iter_mut() {
                label.save(tx).await.unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();

    let mut messages = messages(local_conversation_id, remote_conversation_id, tether).await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            for message in messages.iter_mut() {
                message.save(tx).await.unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();
}

async fn conversations(tether: &Tether) -> Vec<Conversation> {
    let items = RollbackItem::find_by_kind(RollbackItemType::Conversation, tether)
        .await
        .unwrap();

    items
        .into_iter()
        .map(|item| conversation!(remote_id: Some(item.remote_id.into())))
        .collect()
}

async fn messages(
    local_conversation_id: Option<LocalConversationId>,
    remote_conversation_id: Option<ConversationId>,
    tether: &mut Tether,
) -> Vec<Message> {
    let items = RollbackItem::find_by_kind(RollbackItemType::Message, tether)
        .await
        .unwrap();
    let address = create_address(tether).await;

    items
        .into_iter()
        .map(|item| {
            message!(
                remote_id: Some(item.remote_id.into()),
                local_address_id: address.id(),
                remote_address_id: address.remote_id.clone().unwrap(),
                local_conversation_id,
                remote_conversation_id: remote_conversation_id.clone()
            )
        })
        .collect()
}

async fn labels(tether: &Tether) -> Vec<Label> {
    let items = RollbackItem::find_by_kind(RollbackItemType::Label, tether)
        .await
        .unwrap();

    items
        .into_iter()
        .map(|item| {
            label!(
                remote_id: Some(item.remote_id.into())
            )
        })
        .collect()
}

async fn start_server(tether: &Tether, batch_size: usize) -> (MockServer, Session) {
    let mock_server = MockServer::start().await;
    mock_auth_endpoints(&mock_server).await;

    let api = {
        let config = Config {
            env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path("/api")),
            ..Default::default()
        };

        Session::builder()
            .with_config(config)
            .build()
            .await
            .unwrap()
    };

    let kinds = vec![
        RollbackItemType::Conversation,
        RollbackItemType::Message,
        RollbackItemType::Label,
    ];

    for kind in kinds {
        let items = RollbackItem::find_by_kind(kind, tether).await.unwrap();

        if items.is_empty() {
            continue;
        }

        for chunks in items.chunks(batch_size) {
            let items = chunks.to_vec();

            match kind {
                RollbackItemType::Conversation => mock_get_conversation(&mock_server, items).await,
                RollbackItemType::Message => mock_get_message(&mock_server, items, tether).await,
                RollbackItemType::Label => mock_label(&mock_server, items).await,
            }
        }
    }

    (mock_server, api)
}

#[function_name::named]
async fn mock_get_conversation(mock_server: &MockServer, items: Vec<RollbackItem>) {
    let mut api_conversations = Vec::with_capacity(items.len());

    let mut mock = Mock::given(method("GET")).and(path("/api/mail/v4/conversations".to_string()));

    for (index, item) in items.iter().enumerate() {
        mock = mock.and(query_param_contains(
            format!("ID[{index}]"),
            item.remote_id.clone(),
        ));
        api_conversations.push(api_conversation!(id: item.remote_id.clone().into()));
    }

    mock.respond_with(
        ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
            conversations: api_conversations,
            stale: false,
            total: 1,
        }),
    )
    .expect(1)
    .named(function_name!())
    .mount(mock_server)
    .await;
}

#[function_name::named]
async fn mock_get_message(mock_server: &MockServer, items: Vec<RollbackItem>, tether: &Tether) {
    let mut api_metadatas = Vec::with_capacity(items.len());
    let mut mock = Mock::given(method("GET")).and(path("/api/mail/v4/messages"));
    for (index, item) in items.iter().enumerate() {
        let db_message = Message::find_by_remote_id(item.remote_id.clone().into(), tether)
            .await
            .unwrap()
            .unwrap();
        let api_meta = api_message_meta!(
            id: item.remote_id.clone().into(),
            address_id: db_message.remote_address_id.clone(),
            conversation_id: db_message.remote_conversation_id.clone().unwrap()
        );
        api_metadatas.push(api_meta);
        mock = mock.and(query_param_contains(
            format!("ID[{index}]"),
            db_message.remote_id.clone().unwrap().into_inner(),
        ));
    }
    mock.respond_with(
        ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
            messages: api_metadatas,
            stale: false,
            total: 0,
        }),
    )
    .expect(1)
    .named(function_name!())
    .mount(mock_server)
    .await;
}

#[function_name::named]
async fn mock_label(mock_server: &MockServer, items: Vec<RollbackItem>) {
    let remote_ids = items
        .iter()
        .map(|item| LabelId::from(item.remote_id.clone()))
        .collect();
    let api_labels = items
        .into_iter()
        .map(|item| api_label!(id: item.remote_id.clone().into()))
        .collect();
    dbg!(&remote_ids);

    Mock::given(method("POST"))
        .and(path("/api/core/v4/labels/by-ids"))
        .and(body_json(GetLabelsByIdsOptions {
            label_ids: remote_ids,
        }))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetLabelsResponse { labels: api_labels }),
        )
        .expect(1)
        .named(function_name!())
        .mount(mock_server)
        .await;
}
