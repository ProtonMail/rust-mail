mod actions;
mod domain;
mod sources;

pub use actions::*;
pub use domain::*;
use proton_action_queue::{
    ActionFactory, ActionQueue, ActionStore, SessionProvider, SessionProviderError,
};
use proton_api_core::session::Session;
pub use sources::*;
use stash::stash::Stash;
use std::io::stdout;
use std::sync::Arc;
use tracing::subscriber::set_global_default;
use tracing::Level;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{registry, EnvFilter};

pub struct PanicSessionProvider {}

impl SessionProvider for PanicSessionProvider {
    fn retrieve_session(&self) -> Result<Session, SessionProviderError> {
        panic!("should not be called");
    }
}

pub struct TestCtx {
    stash: Stash,
    _file: tempfile::NamedTempFile,
}

impl TestCtx {
    pub async fn new() -> TestCtx {
        let tmp_file = tempfile::NamedTempFile::new().expect("failed to create tempfile");
        drop(set_global_default(
            registry()
                .with(EnvFilter::new(
                    "debug,proton_action_queue=trace,stash=debug",
                ))
                .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
        ));

        tracing::info!("DB crated at {:?}", tmp_file.path());

        let stash = Stash::new(Some(tmp_file.path())).expect("failed to create stash");

        ActionStore::init_tables(&stash)
            .await
            .expect("failed to init store tables");

        let _ = TestLocalSource::new_with_init(stash.clone())
            .await
            .expect("failed to crease local source");
        TestCtx {
            stash,
            _file: tmp_file,
        }
    }

    pub fn stash(&self) -> Stash {
        self.stash.clone()
    }

    pub fn new_action_queue(&self, remote: Arc<dyn RemoteSource>) -> ActionQueue {
        // If action queue fails to initialize, when remote source is mocked it will
        // trigger the mock check before the failure is printed.
        let _remote = remote.clone();

        let factory = build_factory(remote);
        ActionQueue::new(
            self.stash.clone(),
            Box::new(PanicSessionProvider {}),
            factory,
        )
    }
}

pub fn build_factory(remote: Arc<dyn RemoteSource>) -> ActionFactory {
    let mut action_factory = ActionFactory::new();

    action_factory
        .register(Box::new(
            TestActionFactoryInstance::<MoveMessageAction>::new(remote.clone()),
        ))
        .expect("failed to add factory");

    action_factory
        .register(Box::new(
            TestActionFactoryInstance::<DeleteMessageAction>::new(remote.clone()),
        ))
        .expect("failed to add factory");

    action_factory
}

pub fn new_mock_remote<F: FnOnce(&mut MockRemoteSource)>(f: F) -> Arc<dyn RemoteSource> {
    let mut mock = MockRemoteSource::new();
    (f)(&mut mock);
    Arc::new(mock)
}

#[tokio::test]
async fn test_local_source() {
    // Sanity Check local source implementation to ensure tests will work reliably
    let test_ctx = TestCtx::new().await;
    let transaction = test_ctx
        .stash()
        .transaction()
        .await
        .expect("failed to start transaction");
    let mut tx = TestLocalSourceTransaction::new(transaction.clone());
    let folder_id1 = tx
        .create_folder("foo")
        .await
        .expect("failed to create folder");
    let folder_id2 = tx
        .create_folder("bar")
        .await
        .expect("failed to create folder");
    let folder_id3 = tx
        .create_folder("xxx")
        .await
        .expect("failed to create folder");

    let label_id1 = tx.create_label("l1").await.expect("failed to create label");
    let label_id2 = tx.create_label("l2").await.expect("failed to create label");

    let message_id1 = tx
        .create_message(false)
        .await
        .expect("failed to create message");
    let message_id2 = tx
        .create_message(false)
        .await
        .expect("failed to create message");
    let message_id3 = tx
        .create_message(true)
        .await
        .expect("failed to create message");

    tx.move_message_to_folder(&[message_id2], folder_id1)
        .await
        .expect("failed to move");
    tx.move_message_to_folder(&[message_id3], folder_id2)
        .await
        .expect("failed to move");
    {
        let message = tx
            .get_message(message_id1)
            .await
            .expect("failed to get message")
            .expect("message should be found");
        assert_eq!(message.id, message_id1);
        assert!(!message.read);
        assert!(message.folder.is_none());
    }
    {
        let message = tx
            .get_message(message_id2)
            .await
            .expect("failed to get message")
            .expect("message should be found");
        assert_eq!(message.id, message_id2);
        assert!(!message.read);
        assert_eq!(message.folder, Some(folder_id1));
    }
    {
        let messages = tx
            .get_messages(&[message_id1, message_id2, message_id3])
            .await
            .expect("failed to get messages");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].id, message_id1);
        assert_eq!(messages[1].id, message_id2);
        assert_eq!(messages[2].id, message_id3);
    }

    {
        assert!(tx
            .get_message(MessageId(25))
            .await
            .expect("failed to get message")
            .is_none());
    }
    {
        let name = tx
            .get_folder_name(folder_id1)
            .await
            .expect("failed to get folder")
            .expect("folder should be found");
        assert_eq!(name, "foo");
    }
    {
        let name = tx
            .get_folder_name(folder_id2)
            .await
            .expect("failed to get folder")
            .expect("folder should be found");
        assert_eq!(name, "bar");
    }
    {
        let name = tx
            .get_folder_name(folder_id3)
            .await
            .expect("failed to get folder")
            .expect("folder should be found");
        assert_eq!(name, "xxx");
    }

    {
        tx.rename_folder(folder_id3, "renamed")
            .await
            .expect("failed to rename folder");
        let name = tx
            .get_folder_name(folder_id3)
            .await
            .expect("failed to get folder")
            .expect("folder should be found");
        assert_eq!(name, "renamed");
    }

    //delete folder
    {
        tx.delete_folder(folder_id3)
            .await
            .expect("failed to delete folder");
        let message = tx
            .get_message(message_id3)
            .await
            .expect("failed to get message")
            .expect("message should be found");
        assert_eq!(message.folder, Some(folder_id2));
    }

    // add labels
    {
        tx.add_message_to_label(&[message_id3], label_id1)
            .await
            .expect("failed to add to folder");
        tx.add_message_to_label(&[message_id3], label_id2)
            .await
            .expect("failed to add to folder");
        let message = tx
            .get_message(message_id3)
            .await
            .expect("failed to get message")
            .expect("message should be found");
        assert_eq!(message.labels, [label_id1, label_id2]);
    }

    // remove folder
    {
        tx.remove_message_from_label(&[message_id2], label_id2)
            .await
            .expect("failed to remove from folder");
        tx.remove_message_from_label(&[message_id2], label_id1)
            .await
            .expect("failed to remove from folder");
        let message = tx
            .get_message(message_id2)
            .await
            .expect("failed to get message")
            .expect("message should be found");
        assert!(message.labels.is_empty());
    }

    // Read unread
    {
        tx.mark_messages_read(true, &[message_id1])
            .await
            .expect("failed to mark messages as read");
        let message = tx
            .get_message(message_id1)
            .await
            .expect("failed to get message")
            .expect("message should be found");
        assert!(message.read);
    }

    // Delete messages
    {
        tx.delete_message(&[message_id1, message_id2])
            .await
            .expect("failed to delete messages");
    }

    transaction
        .commit()
        .await
        .expect("Failed to execute transaction");
}
