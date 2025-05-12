#![allow(non_snake_case)]

use super::*;
use crate::datatypes::{LabelColor, LabelType};
use proton_mail_test_utils::db::new_test_connection;

const REMOTE_ID: &str = "RemoteID";

pub struct TestCase {
    action: Expand,
    local_id: LocalId,
    stash: Stash,
}

impl TestCase {
    fn con(&self) -> &Stash {
        &self.stash
    }
}

async fn test(expand: bool, expanded: bool) -> TestCase {
    let stash = new_test_connection().await;
    let mut label = create_label(expanded);

    label.save(&stash).await.expect("failed to save label");

    let local_id = label.local_id.expect("local_id should be set");
    let action = if expand {
        Expand::expand(local_id)
    } else {
        Expand::collapse(local_id)
    };

    TestCase {
        action,
        local_id,
        stash,
    }
}

async fn assert_test(test: TestCase, expected: bool, failed: usize) {
    let stash = test.con();
    let label = Label::load(test.local_id, stash)
        .await
        .expect("failed to load label")
        .expect("Label not found");

    assert_eq!(label.expanded, expected, "EXPECTED {failed}");
}

fn create_label(expanded: bool) -> Label {
    let mut label = Label::default();

    label.remote_id = Some(REMOTE_ID.to_string().into());
    label.label_type = LabelType::Folder;
    label.name = "label_name".to_string();
    label.color = LabelColor::black();
    label.expanded = expanded;
    label.display = true;

    label
}

mod apply_local {
    use super::*;
    use proton_action_queue::action::Handler as _;
    use stash::stash::Interface;
    use test_case::test_case;

    const EXPECTED: &[(bool, bool, bool)] = &[
        (true, true, true),    // EXPECTED 0
        (false, true, false),  // EXPECTED 1
        (false, false, false), // EXPECTED 2
        (true, false, true),   // EXPECTED 3
    ];

    #[test_case(1 ; "Test1 apply once")]
    #[test_case(2 ; "Test2 apply twice")]
    #[test_case(3 ; "Test3 apply thrice")]
    #[tokio::test]
    async fn test_apply(apply: usize) {
        for (idx, (expand, expanded, expected)) in EXPECTED.iter().enumerate() {
            let mut test = test(*expand, *expanded).await;

            let tx = test.con().transaction().await.unwrap();
            for _ in 0..apply {
                Handler
                    .apply_local(&mut test.action, &tx)
                    .await
                    .expect("failed to apply local");
            }
            tx.commit().await.unwrap();

            assert_test(test, *expected, idx).await;
        }
    }
}

mod revert_local {
    use super::*;
    use proton_action_queue::action::Handler as _;
    use stash::stash::Interface;
    use test_case::test_case;

    const EXPECTED: &[(bool, bool, bool)] = &[
        (true, true, true),    // EXPECTED 0
        (false, true, true),   // EXPECTED 1
        (false, false, false), // EXPECTED 2
        (true, false, false),  // EXPECTED 3
    ];

    #[test_case(0, 1; "Test1 only revert once")]
    #[test_case(0, 2; "Test2 only revert twice")]
    #[test_case(1, 1; "Test3 apply once revert once")]
    #[test_case(1, 2; "Test4 apply once revert twice")]
    #[tokio::test]
    async fn test_revert(apply: usize, revert: usize) {
        for (idx, (expand, expanded, expected)) in EXPECTED.iter().enumerate() {
            let mut test = test(*expand, *expanded).await;

            let tx = test.con().transaction().await.unwrap();
            for _ in 0..apply {
                Handler
                    .apply_local(&mut test.action, &tx)
                    .await
                    .expect("failed to apply local");
            }

            for _ in 0..revert {
                Handler
                    .revert_local(&mut test.action, &tx)
                    .await
                    .expect("failed to apply local");
            }
            tx.commit().await.unwrap();

            assert_test(test, *expected, idx).await;
        }
    }
}

mod apply_remote {
    use super::*;
    use proton_action_queue::action::Handler as _;
    use proton_core_api::services::proton::{response_data::ApiErrorInfo, Config};
    use proton_mail_api::services::proton::{
        requests::PatchLabelRequest, response_data::OperationResult, responses::PatchLabelResponse,
    };
    use stash::stash::Interface;
    use test_case::test_case;
    use wiremock::{
        matchers::{body_json, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[function_name::named]
    pub async fn mock_patch_label(
        mock_server: &MockServer,
        remote_id: &str,
        expanded: bool,
        status: u16,
        code: u32,
        expect: u64,
    ) {
        Mock::given(method("PATCH"))
            .and(path(format!("/api/core/v4/labels/{remote_id}")))
            .and(body_json(PatchLabelRequest {
                expanded: Some(expanded),
                ..Default::default()
            }))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(PatchLabelResponse {
                    responses: vec![OperationResult {
                        id: remote_id.into(),
                        response: ApiErrorInfo {
                            code,
                            ..Default::default()
                        },
                    }],
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(mock_server)
            .await;
    }

    #[test_case(true, true, 1, 1, 0, 200, 1000; "TEST1 apply remote when not modified locally")]
    #[test_case(true, false, 1, 1, 1, 200, 1000; "TEST2 apply remote when modified locally")]
    #[test_case(true, false, 1, 1, 1, 200, 500; "TEST3 apply remote when response body is not 1000 success")]
    #[test_case(true, false, 1, 1, 1, 503, 0 => panics "HTTP status server error (503 Service Unavailable)"; "TEST4 apply remote service unavailable")]
    #[test_case(true, false, 0, 1, 1, 200, 1000; "TEST5 apply remote when not modified locally")]
    #[tokio::test]
    async fn test_remote(
        expand: bool,
        expanded: bool,
        apply_local: usize,
        apply_remote: usize,
        remote_calls: u64,
        http_status: u16,
        response_code: u32,
    ) {
        let mut test = test(expand, expanded).await;
        let mock_server = MockServer::start().await;
        let api_config = Config {
            base_url: format!("{}/api/", mock_server.uri()),
            allow_http: true,
            skip_srp_proof_validation: true,
            ..Default::default()
        };
        let session = Session::new(api_config, None).await.unwrap();
        mock_patch_label(
            &mock_server,
            REMOTE_ID,
            expand,
            http_status,
            response_code,
            remote_calls,
        )
        .await;

        let tx = test.con().transaction().await.unwrap();
        for _ in 0..apply_local {
            Handler
                .apply_local(&mut test.action, &tx)
                .await
                .expect("failed to apply local");
        }
        tx.commit().await.unwrap();

        for _ in 0..apply_remote {
            let stash = test.con().clone();
            Handler
                .apply_remote(&mut test.action, &session, &stash)
                .await
                .unwrap();
        }
    }
}
