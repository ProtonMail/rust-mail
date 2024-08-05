use crate::models::Label;
use crate::{actions::ActionError, AppError};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::LabelId;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Stash, Tether};
use tracing::debug;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Expand {
    local_id: u64,
    remote_id: Option<LabelId>,
    expand: bool,
    original_state: Option<bool>,
}

impl Expand {
    #[allow(clippy::self_named_constructors)]
    pub fn expand(local_id: u64) -> Self {
        Self::new(local_id, true)
    }

    pub fn collapse(local_id: u64) -> Self {
        Self::new(local_id, false)
    }

    fn new(local_id: u64, expand: bool) -> Self {
        Self {
            local_id,
            expand,
            remote_id: None,
            original_state: None,
        }
    }
}

impl Action for Expand {
    const TYPE: Type = Type("expand_label");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler;

impl proton_action_queue::action::Handler for Handler {
    type Action = Expand;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let mut label = Label::load_using(action.local_id, tx)
            .await?
            .ok_or_else(|| AppError::LabelNotFound(action.local_id))?;

        action.original_state = action.original_state.or(Some(label.expanded));

        let label_is_equal_action = action
            .original_state
            .filter(|original_state| *original_state == action.expand)
            .filter(|_| label.expanded == action.expand)
            .is_some();

        if label_is_equal_action {
            debug!(
                "No need to apply expand action for label: {:?}",
                action.local_id
            );

            return Ok(());
        }

        action.remote_id.clone_from(&label.remote_id);

        label.expanded = action.expand;

        label.save_using(tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let Some(original_state) = action
            .original_state
            .filter(|original_state| *original_state != action.expand)
        else {
            return Ok(());
        };

        action.expand = original_state;

        self.apply_local(action, tx).await
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let action_equal_original_state = action
            .original_state
            .filter(|original_state| *original_state == action.expand)
            .is_some();

        if action_equal_original_state {
            return Ok(());
        }

        let remote_id = match action.remote_id.clone() {
            Some(remote_id) => remote_id,
            None => {
                let tx = stash.connection();
                let label = Label::load_using(action.local_id, &tx)
                    .await?
                    .ok_or_else(|| AppError::LabelNotFound(action.local_id))?;

                action.original_state = Some(label.expanded);

                let label_is_equal_action = action
                    .original_state
                    .filter(|_| label.expanded == action.expand)
                    .is_some();

                if label_is_equal_action {
                    return Ok(()); // Nothing to do
                }

                label
                    .remote_id
                    .clone()
                    .ok_or_else(|| AppError::LabelDoesNotHaveRemoteId(action.local_id))?
            }
        };

        Label::patch_expanded(remote_id, action.expand, session.api()).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datatypes::{LabelColor, LabelType};
    use crate::db::new_test_connection;

    const REMOTE_ID: &str = "RemoteID";

    pub struct TestCase {
        action: Expand,
        local_id: u64,
        stash: Stash,
    }

    impl TestCase {
        fn con(&self) -> Tether {
            self.stash.connection()
        }
    }

    async fn test(expand: bool, expanded: bool) -> TestCase {
        let stash = new_test_connection().await;
        let tx = stash.connection();
        let mut label = create_label(expanded);

        label.save_using(&tx).await.expect("failed to save label");

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
        let tx = test.con();
        let label = Label::load_using(test.local_id, &tx)
            .await
            .expect("failed to load label")
            .expect("Label not found");

        assert_eq!(label.expanded, expected, "EXPECTED {failed}");
    }

    fn create_label(expanded: bool) -> Label {
        let mut label = Label::default();

        label.remote_id = Some(REMOTE_ID.to_string().into());
        label.label_type = LabelType::Folder;
        label.name = format!("label_name");
        label.color = LabelColor::black();
        label.expanded = expanded;
        label.display = true;

        label
    }

    mod apply_local {
        use super::*;
        use proton_action_queue::action::Handler as _;
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
                let tx = test.con();

                for _ in 0..apply {
                    Handler::default()
                        .apply_local(&mut test.action, &tx)
                        .await
                        .expect("failed to apply local");
                }

                assert_test(test, *expected, idx).await;
            }
        }
    }

    mod revert_local {
        use super::*;
        use proton_action_queue::action::Handler as _;
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
                let tx = test.con();

                for _ in 0..apply {
                    Handler::default()
                        .apply_local(&mut test.action, &tx)
                        .await
                        .expect("failed to apply local");
                }

                for _ in 0..revert {
                    Handler::default()
                        .revert_local(&mut test.action, &tx)
                        .await
                        .expect("failed to apply local");
                }

                assert_test(test, *expected, idx).await;
            }
        }
    }

    mod apply_remote {
        use super::*;
        use proton_action_queue::action::Handler as _;
        use proton_api_core::services::proton::{response_data::ApiErrorInfo, Config};
        use proton_api_mail::services::proton::{
            requests::PatchLabelRequest, response_data::OperationResult,
            responses::PatchLabelResponse,
        };
        use test_case::test_case;
        use wiremock::{
            matchers::{body_json, method, path},
            Mock, MockServer, ResponseTemplate,
        };

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
            let tx = test.con();
            let mock_server = MockServer::start().await;
            let api_config = Config {
                base_url: format!("{}/api/", mock_server.uri()),
                allow_http: true,
                skip_srp_proof_validation: true,
                ..Default::default()
            };
            let session = Session::new(api_config);
            mock_patch_label(
                &mock_server,
                REMOTE_ID,
                expand,
                http_status,
                response_code,
                remote_calls,
            )
            .await;

            for _ in 0..apply_local {
                Handler::default()
                    .apply_local(&mut test.action, &tx)
                    .await
                    .expect("failed to apply local");
            }

            for _ in 0..apply_remote {
                Handler::default()
                    .apply_remote(&mut test.action, &session, &test.stash)
                    .await
                    .unwrap();
            }
        }
    }
}
