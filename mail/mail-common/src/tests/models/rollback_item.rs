use crate as proton_mail_common;
use proton_api_core::services::proton::{Config, Proton};
#[allow(unused_imports)]
use proton_api_mail::{
    services::proton::{
        requests::{GetConversationsOptions, GetLabelsByIdsOptions},
        responses::{GetConversationsResponse, GetLabelsResponse, GetMessageResponse},
    },
    MAX_LIMIT_VALUE_U64, MAX_PAGE_ELEMENT_COUNT_U64,
};
use proton_core_common::{datatypes::LocalId, models::ModelExtension};
use proton_mail_test_utils::db::new_test_connection_file;
use proton_mail_test_utils::{
    api_conversation, api_label, api_message, api_message_meta, common::create_address,
    conversation, label, message,
};
use test_case::test_case;
#[allow(unused_imports)]
use wiremock::{
    matchers::{body_json, method, path, query_param_contains},
    Mock, MockServer, ResponseTemplate,
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
    let tx = stash.connection();

    for item in expected.iter_mut().flat_map(|x| x.iter_mut()) {
        item.set_stash(&stash);
        item.save().await.unwrap();
    }

    for item in input.iter_mut() {
        item.set_stash(&stash);
        item.save().await.unwrap();
    }

    let expected = expected.unwrap_or(input);
    let actual = RollbackItem::all(&stash, None).await.unwrap();

    assert_eq!(expected, actual);

    // * RollbackItem is correctly synced *
    setup_database(&tx).await;
    let (_mock, api) = start_server(&stash).await;

    RollbackItem::sync_all(&api, &stash, 2).await.unwrap();

    // * RollbackItems are correctly deleted during sync *
    let actual = RollbackItem::all(&stash, None).await.unwrap();

    assert_eq!(actual.len(), 0);

    // * RollbackItems with no limit for empty stash *
    RollbackItem::sync_all(&api, &stash, None).await.unwrap();
}

async fn setup_database(tx: &Tether) {
    let mut conversations = conversations(tx).await;
    let mut local_conversation_id = None;
    let mut remote_conversation_id = None;

    for conversation in conversations.iter_mut() {
        conversation.save_using(tx).await.unwrap();
        local_conversation_id = conversation.local_id;
        remote_conversation_id = conversation.remote_id.clone();
    }

    let mut labels = labels(tx).await;

    for label in labels.iter_mut() {
        label.save_using(tx).await.unwrap();
    }

    let mut messages = messages(local_conversation_id, remote_conversation_id, tx).await;

    for message in messages.iter_mut() {
        message.save_using(tx).await.unwrap();
    }
}

async fn conversations(tx: &Tether) -> Vec<Conversation> {
    let items = RollbackItem::find_by_kind(RollbackItemType::Conversation, tx)
        .await
        .unwrap();

    items
        .into_iter()
        .map(|item| conversation!(remote_id: Some(dbg!( item.remote_id )), stash: item.stash))
        .collect()
}

async fn messages(
    local_conversation_id: Option<LocalId>,
    remote_conversation_id: Option<RemoteId>,
    tx: &Tether,
) -> Vec<Message> {
    let items = RollbackItem::find_by_kind(RollbackItemType::Message, tx)
        .await
        .unwrap();
    let address = create_address(tx).await;

    items
        .into_iter()
        .map(|item| {
            message!(
                remote_id: Some(item.remote_id),
                stash: item.stash,
                local_address_id: address.local_id.unwrap(),
                remote_address_id: address.remote_id.clone().unwrap(),
                local_conversation_id,
                remote_conversation_id: remote_conversation_id.clone()
            )
        })
        .collect()
}

async fn labels(tx: &Tether) -> Vec<Label> {
    let items = RollbackItem::find_by_kind(RollbackItemType::Label, tx)
        .await
        .unwrap();

    items
        .into_iter()
        .map(|item| {
            label!(
                remote_id: Some(item.remote_id.into()),
                stash: item.stash
            )
        })
        .collect()
}

async fn start_server(stash: &Stash) -> (MockServer, Proton) {
    let mock_server = MockServer::start().await;
    let api_config = Config {
        base_url: format!("{}/api/", mock_server.uri()),
        allow_http: true,
        skip_srp_proof_validation: true,
        ..Default::default()
    };
    let api = Proton::new(api_config, None, None).await.unwrap();
    let kinds = vec![
        RollbackItemType::Conversation,
        RollbackItemType::Message,
        RollbackItemType::Label,
    ];

    for kind in kinds {
        let items = RollbackItem::find_by_kind(kind, stash).await.unwrap();

        for item in items {
            dbg!(&item);
            match item.item_type {
                RollbackItemType::Conversation => mock_get_conversation(&mock_server, &item).await,
                RollbackItemType::Message => mock_get_message(&mock_server, &item, stash).await,
                RollbackItemType::Label => mock_label(&mock_server, &item).await,
            }
        }
    }

    (mock_server, api)
}

async fn mock_get_conversation(mock_server: &MockServer, item: &RollbackItem) {
    let remote_id = &item.remote_id;
    let api_conversation = api_conversation!(id: remote_id.clone().into());

    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations".to_string()))
        .and(query_param_contains("ID[0]", remote_id.to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                conversations: vec![api_conversation],
                stale: false,
                total: 1,
            }),
        )
        .expect(1)
        .mount(mock_server)
        .await;
}

async fn mock_get_message(mock_server: &MockServer, item: &RollbackItem, stash: &Stash) {
    let remote_id = &item.remote_id;
    let db_message = Message::find_by_id(remote_id.clone(), stash)
        .await
        .unwrap()
        .unwrap();
    let api_meta = api_message_meta!(
        id: remote_id.clone().into(),
        address_id: db_message.remote_address_id.clone().into(),
        conversation_id: db_message.remote_conversation_id.clone().unwrap().into()
    );
    let api_message = api_message!(
        metadata: api_meta
    );

    Mock::given(method("GET"))
        .and(path(format!("/api/mail/v4/messages/{remote_id}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetMessageResponse {
                message: api_message,
            }),
        )
        .expect(1)
        .mount(mock_server)
        .await;
}

async fn mock_label(mock_server: &MockServer, item: &RollbackItem) {
    let remote_id = &item.remote_id;
    let api_label = api_label!(id: remote_id.clone().into());

    Mock::given(method("POST"))
        .and(path("/api/core/v4/labels/by-ids".to_string()))
        .and(body_json(GetLabelsByIdsOptions {
            label_ids: vec![remote_id.clone().into()],
        }))
        .respond_with(ResponseTemplate::new(200).set_body_json(GetLabelsResponse {
            labels: vec![api_label],
        }))
        .expect(1)
        .mount(mock_server)
        .await;
}
