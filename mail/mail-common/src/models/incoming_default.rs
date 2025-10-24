use anyhow::anyhow;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use stash::orm::ModelHooks;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Instant;

use indoc::indoc;
use proton_core_api::session::Session;
use proton_core_common::datatypes::InitializationKey;
use proton_core_common::models::Address;
use proton_core_common::models::InitializationError;
use proton_core_common::models::InitializationWatcher;
use proton_core_common::models::InitializedComponent;
use proton_mail_api::INCOMING_DEFAULTS_PAGE_SIZE;
use proton_mail_api::services::proton::ProtonMail;
use proton_task_service::BackgroundAwareTaskService;
use stash::exports::Transaction;

use derive_more::TryFrom;
use proton_action_queue::queue::ActionError as QueueActionError;
use proton_action_queue::queue::Queue;
use proton_action_queue::queue::QueuedActionOutput;
use proton_core_api::services::proton::IncomingDefaultId;
use proton_core_api::services::proton::PrivateEmail;
use stash::exports::FromSql;
use stash::exports::FromSqlError;
use stash::exports::SqliteError;
use stash::exports::ToSql;
use stash::exports::ToSqlOutput;
use stash::exports::Value;
use stash::macros::Model;

use proton_mail_api::services::proton::response_data::IncomingDefault as ApiIncomingDefault;
use proton_mail_api::services::proton::response_data::IncomingDefaultEvent as ApiIncomingDefaultEvent;
use proton_mail_api::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use stash::orm::Model;
use stash::params;
use stash::stash::Bond;
use stash::stash::Stash;
use stash::stash::StashError;
use stash::stash::Tether;

use crate::MailContextError;
use crate::actions::MailActionError;
use crate::actions::addresses::block::Block;
use crate::actions::addresses::unblock::Unblock;
use crate::actions::addresses::update_incoming_defaults::SyncIncomingDefaults;
use crate::datatypes::LocalIncomingDefaultId;

#[derive(Clone, PartialEq, Debug, Eq)]
pub struct IncomingDefaultEvent {
    pub remote_id: IncomingDefaultId,
}

impl From<ApiIncomingDefaultEvent> for IncomingDefaultEvent {
    fn from(api: ApiIncomingDefaultEvent) -> Self {
        let ApiIncomingDefaultEvent { id, action: _ } = api;

        IncomingDefaultEvent {
            remote_id: id.into(),
        }
    }
}

#[derive(Clone, Debug, Model, PartialEq)]
#[TableName("incoming_defaults")]
#[ModelHooks]
pub struct IncomingDefault {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalIncomingDefaultId>,

    #[DbField]
    pub remote_id: Option<IncomingDefaultId>,

    #[DbField]
    pub email: PrivateEmail,

    #[DbField]
    pub location: IncomingDefaultLocation,

    #[DbField]
    pub domain: Option<String>,

    #[DbField]
    pub deleted: bool,
}

impl ModelHooks for IncomingDefault {
    fn after_load(&mut self, _: &stash::exports::Connection) -> stash::stash::StashResult<()> {
        Ok(())
    }

    fn before_save(&mut self, _: &Transaction<'_>) -> stash::stash::StashResult<()> {
        self.email = PrivateEmail::new(Self::sanitize(self.email.as_clear_text_str()));
        Ok(())
    }

    fn after_save(&mut self, _: &Transaction<'_>) -> stash::stash::StashResult<()> {
        Ok(())
    }
}

impl From<ApiIncomingDefault> for IncomingDefault {
    fn from(api: ApiIncomingDefault) -> Self {
        let ApiIncomingDefault {
            location,
            action: _,
            email,
            id,
            domain,
        } = api;

        Self {
            local_id: None,
            remote_id: Some(id.into()),
            email: email.expect("email is required"),
            location: location.into(),
            domain,
            deleted: false,
        }
    }
}

static SANITIZE_EMAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^a-zA-Z0-9!#$%&'*+\-=?^_`{|}~@.\[\]]+").unwrap());

impl IncomingDefault {
    /// This sanitization replicates of what we do on the server side (except for trimming emails to 191 characters cause that's just silly).
    /// See: https://www.php.net/manual/en/filter.constants.php#constant.filter-sanitize-email
    fn sanitize(email: impl Into<String>) -> String {
        let email = email.into();
        let email = SANITIZE_EMAIL_REGEX.replace_all(&email, "").to_string();
        email.to_lowercase()
    }

    pub async fn by_email(
        email: impl Into<String>,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let email = Self::sanitize(email);
        Self::find_first("WHERE email = ? AND deleted = 0", params![email], tether).await
    }

    pub async fn update_from_api(
        local_id: LocalIncomingDefaultId,
        api: ApiIncomingDefault,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let incoming: Self = api.into();
        Self {
            local_id: Some(local_id),
            ..incoming
        }
        .save(bond)
        .await?;
        Ok(())
    }

    pub async fn update_location(
        local_id: LocalIncomingDefaultId,
        location: IncomingDefaultLocation,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            format!(
                "UPDATE {} SET location = ? WHERE local_id = ?",
                Self::table_name()
            ),
            params![location, local_id],
        )
        .await?;
        Ok(())
    }

    pub async fn replace_all(new: Vec<Self>, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.sync_bridge(move |tx| Self::replace_all_sync(new, tx))
            .await?;
        Ok(())
    }

    pub fn replace_all_sync(new: Vec<Self>, tx: &Transaction<'_>) -> Result<(), StashError> {
        tx.execute("DELETE FROM incoming_defaults", ())?;
        Self::save_all_sync(new, tx)?;
        Ok(())
    }

    fn save_all_sync(new: Vec<Self>, tx: &Transaction<'_>) -> Result<(), StashError> {
        let mut q = tx.prepare_cached(indoc! {"
            INSERT INTO incoming_defaults
                (email, location, domain, remote_id)
            VALUES (?, ?, ?, ?);
        "})?;
        for incoming in new {
            q.execute((
                Self::sanitize(incoming.email.as_clear_text_str()),
                incoming.location,
                incoming.domain,
                incoming.remote_id,
            ))?;
        }
        Ok(())
    }

    pub async fn action_block(
        queue: &Queue,
        email: PrivateEmail,
    ) -> Result<QueuedActionOutput<Block>, QueueActionError<Block>> {
        let action = Block::new(email);
        queue.queue_action(action).await
    }

    pub async fn action_unblock(
        queue: &Queue,
        email: PrivateEmail,
    ) -> Result<QueuedActionOutput<Unblock>, QueueActionError<Unblock>> {
        let action = Unblock::new(email);
        queue.queue_action(action).await
    }

    pub async fn action_resync(queue: &Queue) {
        if let Err(e) = queue.queue_action(SyncIncomingDefaults).await {
            if cfg!(debug_assertions) {
                panic!("apply_local can't fail {e}");
            } else {
                tracing::error!(?e);
            }
        }
    }
}

/// Where do messages from a sender go by default. This is handled by the backend, but we sometimes
/// want this informaton for things like banners.
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFrom, Serialize, Deserialize)]
#[try_from(repr)]
#[repr(u8)]
pub enum IncomingDefaultLocation {
    /// The messages are allowed and go to inbox
    /// Email marked initially as spam by Proton, but marked as "OK" by the user.
    Inbox = 0,
    /// Marked as spam by the user, next incoming messages goes to spam directly
    Spam = 4,
    /// email address blocked by the user, going to permanent deleted immediately (not to trash, not to spam)
    /// The messages are not received and are deleted automatically
    Blocked = 14,
}
impl From<ApiIncomingDefaultLocation> for IncomingDefaultLocation {
    fn from(value: ApiIncomingDefaultLocation) -> Self {
        match value {
            ApiIncomingDefaultLocation::Inbox => Self::Inbox,
            ApiIncomingDefaultLocation::Spam => Self::Spam,
            ApiIncomingDefaultLocation::Blocked => Self::Blocked,
        }
    }
}

impl FromSql for IncomingDefaultLocation {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for IncomingDefaultLocation {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl IncomingDefault {
    pub const INIT_KEY: InitializationKey = InitializationKey::new("incoming_defaults");

    /// Idempotently initialization, syncing with the backend.
    pub async fn initialize(
        watcher: Arc<InitializationWatcher>,
        api: &Session,
        stash: &Stash,
        tasks: &BackgroundAwareTaskService,
    ) -> Result<(), InitializationError<MailContextError>> {
        InitializedComponent::initialize(
            watcher,
            Self::INIT_KEY,
            &[Address::INIT_KEY],
            stash.connection().await?,
            async || Ok(Self::sync(api, tasks).await?),
            |tx, res| {
                Self::replace_all_sync(res.into_iter().map(Into::into).collect(), tx)?;
                Ok(())
            },
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn sync(
        api: &Session,
        tasks_service: &BackgroundAwareTaskService,
    ) -> Result<Vec<ApiIncomingDefault>, MailActionError> {
        let t0 = Instant::now();
        let initial = api.get_incoming_defaults(0).await?;
        tracing::debug!("Requested initial batch in {:?}", t0.elapsed());

        let page = INCOMING_DEFAULTS_PAGE_SIZE;
        let mut tasks = vec![];
        if let Some(rem) = initial.global_total.checked_sub(page) {
            let rem = rem.div_ceil(page);
            tracing::debug!("Requesting {rem} batches for contacts");
            for page in 1..=rem {
                let api = api.clone();
                let task = tasks_service.spawn(async move {
                    api.get_incoming_defaults(page)
                        .await
                        .map(|x| x.incoming_defaults)
                });
                tasks.push(task);
            }
        }
        tracing::debug!("Requested all batches in {:?}", t0.elapsed());

        let ret = futures::future::join_all(tasks).await;

        let mut out = vec![];

        for defs in std::iter::once(Ok(Ok(initial.incoming_defaults))).chain(ret) {
            out.extend(
                defs.map_err(|e| MailActionError::Other(anyhow!("Failed to join task: {}", e)))??,
            );
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("" => "".to_string(); "empty")]
    #[test_case("valid@email.com" => "valid@email.com".to_string(); "normal")]
    #[test_case("VaLiD@email.com" => "valid@email.com".to_string(); "mixed case")]
    #[test_case("999@email.com" => "999@email.com".to_string(); "valid number")]
    #[test_case("test!@email.com" => "test!@email.com".to_string(); "valid exclamation mark")]
    #[test_case("test#@email.com" => "test#@email.com".to_string(); "valid hash")]
    #[test_case("test$@email.com" => "test$@email.com".to_string(); "valid dollar")]
    #[test_case("test%@email.com" => "test%@email.com".to_string(); "valid percent")]
    #[test_case("test&@email.com" => "test&@email.com".to_string(); "valid ampersand")]
    #[test_case("test'@email.com" => "test'@email.com".to_string(); "valid apostrophe")]
    #[test_case("test*@email.com" => "test*@email.com".to_string(); "valid asterisk")]
    #[test_case("test+@email.com" => "test+@email.com".to_string(); "valid plus")]
    #[test_case("test-@email.com" => "test-@email.com".to_string(); "valid minus")]
    #[test_case("test=@email.com" => "test=@email.com".to_string(); "valid equals")]
    #[test_case("test?@email.com" => "test?@email.com".to_string(); "valid question")]
    #[test_case("test^@email.com" => "test^@email.com".to_string(); "valid caret")]
    #[test_case("test_@email.com" => "test_@email.com".to_string(); "valid underscore")]
    #[test_case("test`@email.com" => "test`@email.com".to_string(); "valid backtick")]
    #[test_case("test{@email.com" => "test{@email.com".to_string(); "valid left brace")]
    #[test_case("test|@email.com" => "test|@email.com".to_string(); "valid pipe")]
    #[test_case("test}@email.com" => "test}@email.com".to_string(); "valid right brace")]
    #[test_case("test~@email.com" => "test~@email.com".to_string(); "valid tilde")]
    #[test_case("test[@email.com" => "test[@email.com".to_string(); "valid left bracket")]
    #[test_case("test]@email.com" => "test]@email.com".to_string(); "valid right bracket")]
    #[test_case("test<>@email.com" => "test@email.com".to_string(); "sanitize angle brackets")]
    #[test_case("test()@email.com" => "test@email.com".to_string(); "sanitize parentheses")]
    #[test_case("test,@email.com" => "test@email.com".to_string(); "sanitize comma")]
    #[test_case("test;@email.com" => "test@email.com".to_string(); "sanitize semicolon")]
    #[test_case("test:@email.com" => "test@email.com".to_string(); "sanitize colon")]
    #[test_case("test\"@email.com" => "test@email.com".to_string(); "sanitize quote")]
    #[test_case("test/@email.com" => "test@email.com".to_string(); "sanitize slash")]
    #[test_case("test\\@email.com" => "test@email.com".to_string(); "sanitize backslash")]
    #[test_case("test @email.com" => "test@email.com".to_string(); "sanitize space")]
    #[test_case("test\t@email.com" => "test@email.com".to_string(); "sanitize tab")]
    #[test_case("test\n@email.com" => "test@email.com".to_string(); "sanitize newline")]
    #[test_case("tëst@émàil.com" => "tst@mil.com".to_string(); "sanitize unicode")]
    #[test_case("test😀@email.com" => "test@email.com".to_string(); "sanitize emoji")]
    #[test_case("123!@#$%&*+-=?^_`{|}~[]aBc@domain.com" => "123!@#$%&*+-=?^_`{|}~[]abc@domain.com".to_string(); "all valid characters")]
    fn sanitize_emails(email: &str) -> String {
        IncomingDefault::sanitize(email)
    }
}
