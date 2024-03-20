mod actions;
mod domain;
mod sources;

pub use actions::*;
pub use domain::*;
use proton_action_queue::{
    ActionFactory, ActionQueue, ActionStore, DefaultSqlConnectionProvider, SessionProvider,
    SessionProviderError,
};
use proton_api_core::Session;
use proton_sqlite3::{InProcessTrackerService, SqliteConnectionPool, SqliteMode};
pub use sources::*;
use std::sync::Arc;

pub struct PanicSessionProvider {}

impl SessionProvider for PanicSessionProvider {
    fn retrieve_session(&self) -> Result<Session, SessionProviderError> {
        panic!("should not be called");
    }
}

pub struct TestCtx {
    tracker: InProcessTrackerService,
    _file: tempfile::NamedTempFile,
    _tracing_guard: tracing::dispatcher::DefaultGuard,
}

impl TestCtx {
    pub fn new() -> TestCtx {
        let tmp_file = tempfile::NamedTempFile::new().expect("failed to create tempfile");
        // a builder for `FmtSubscriber`.
        let subscriber = tracing_subscriber::FmtSubscriber::builder()
            // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
            // will be written to stdout.
            .with_max_level(tracing::Level::TRACE)
            // completes the builder.
            .finish();

        let guard = tracing::subscriber::set_default(subscriber);
        tracing::info!("DB crated at {:?}", tmp_file.path());

        let pool = SqliteConnectionPool::new(SqliteMode::File(tmp_file.path().to_path_buf()), true);
        let tracker = InProcessTrackerService::new(pool).expect("failed to create tracker");

        {
            let mut conn = tracker.new_connection().unwrap();
            ActionStore::init_tables(conn.as_mut()).expect("failed to init store tables");
        }

        let _ = TestLocalSource::new_with_init(&tracker).expect("failed to crease local source");
        TestCtx {
            tracker,
            _file: tmp_file,
            _tracing_guard: guard,
        }
    }

    pub fn tx<R, F: Fn(TestLocalSourceTransaction) -> R>(&mut self, f: F) -> R {
        let mut source =
            TestLocalSource::new(&self.tracker).expect("failed to create local source");
        let r = source
            .tx(move |tx| -> Result<R, proton_sqlite3::rusqlite::Error> { Ok((f)(tx)) })
            .expect("failed to execute");
        r
    }

    pub fn new_action_queue(&self, remote: Arc<dyn RemoteSource>) -> ActionQueue {
        // If action queue fails to initialize, when remote source is mocked it will
        // trigger the mock check before the failure is printed.
        let _remote = remote.clone();

        let factory = build_factory(remote);
        ActionQueue::new(
            Box::new(DefaultSqlConnectionProvider::new(self.tracker.clone())),
            Box::new(PanicSessionProvider {}),
            factory,
        )
        .expect("failed to build queue")
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

#[test]
fn test_local_source() {
    // Sanity Check local source implementation to ensure tests will work reliably
    let mut test_ctx = TestCtx::new();

    test_ctx
        .tx(|mut tx| -> Result<(), proton_sqlite3::rusqlite::Error> {
            let folder_id1 = tx.create_folder("foo").expect("failed to create folder");
            let folder_id2 = tx.create_folder("bar").expect("failed to create folder");
            let folder_id3 = tx.create_folder("xxx").expect("failed to create folder");

            let label_id1 = tx.create_label("l1").expect("failed to create label");
            let label_id2 = tx.create_label("l2").expect("failed to create label");

            let message_id1 = tx.create_message(false).expect("failed to create message");
            let message_id2 = tx.create_message(false).expect("failed to create message");
            let message_id3 = tx.create_message(true).expect("failed to create message");

            tx.move_message_to_folder(&[message_id2], folder_id1)
                .expect("failed to move");
            tx.move_message_to_folder(&[message_id3], folder_id2)
                .expect("failed to move");
            {
                let message = tx
                    .get_message(message_id1)
                    .expect("failed to get message")
                    .expect("message should be found");
                assert_eq!(message.id, message_id1);
                assert!(!message.read);
                assert!(message.folder.is_none());
            }
            {
                let message = tx
                    .get_message(message_id2)
                    .expect("failed to get message")
                    .expect("message should be found");
                assert_eq!(message.id, message_id2);
                assert!(!message.read);
                assert_eq!(message.folder, Some(folder_id1));
            }
            {
                let messages = tx
                    .get_messages(&[message_id1, message_id2, message_id3])
                    .expect("failed to get messages");
                assert_eq!(messages.len(), 3);
                assert_eq!(messages[0].id, message_id1);
                assert_eq!(messages[1].id, message_id2);
                assert_eq!(messages[2].id, message_id3);
            }

            {
                assert!(tx
                    .get_message(MessageId(25))
                    .expect("failed to get message")
                    .is_none());
            }
            {
                let name = tx
                    .get_folder_name(folder_id1)
                    .expect("failed to get folder")
                    .expect("folder should be found");
                assert_eq!(name, "foo");
            }
            {
                let name = tx
                    .get_folder_name(folder_id2)
                    .expect("failed to get folder")
                    .expect("folder should be found");
                assert_eq!(name, "bar");
            }
            {
                let name = tx
                    .get_folder_name(folder_id3)
                    .expect("failed to get folder")
                    .expect("folder should be found");
                assert_eq!(name, "xxx");
            }

            {
                tx.rename_folder(folder_id3, "renamed")
                    .expect("failed to rename folder");
                let name = tx
                    .get_folder_name(folder_id3)
                    .expect("failed to get folder")
                    .expect("folder should be found");
                assert_eq!(name, "renamed");
            }

            //delete folder
            {
                tx.delete_folder(folder_id3)
                    .expect("failed to delete folder");
                let message = tx
                    .get_message(message_id3)
                    .expect("failed to get message")
                    .expect("message should be found");
                assert_eq!(message.folder, Some(folder_id2));
            }

            // add labels
            {
                tx.add_message_to_label(&[message_id3], label_id1)
                    .expect("failed to add to folder");
                tx.add_message_to_label(&[message_id3], label_id2)
                    .expect("failed to add to folder");
                let message = tx
                    .get_message(message_id3)
                    .expect("failed to get message")
                    .expect("message should be found");
                assert_eq!(message.labels, [label_id1, label_id2]);
            }

            // remove folder
            {
                tx.remove_message_from_label(&[message_id2], label_id2)
                    .expect("failed to remove from folder");
                tx.remove_message_from_label(&[message_id2], label_id1)
                    .expect("failed to remove from folder");
                let message = tx
                    .get_message(message_id2)
                    .expect("failed to get message")
                    .expect("message should be found");
                assert!(message.labels.is_empty());
            }

            // Read unread
            {
                tx.mark_messages_read(true, &[message_id1])
                    .expect("failed to mark messages as read");
                let message = tx
                    .get_message(message_id1)
                    .expect("failed to get message")
                    .expect("message should be found");
                assert!(message.read);
            }

            // Delete messages
            {
                tx.delete_message(&[message_id1, message_id2])
                    .expect("failed to delete messages");
            }

            Ok(())
        })
        .expect("Failed to execute transaction");
}
