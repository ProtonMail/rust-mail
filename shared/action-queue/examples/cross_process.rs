//! Simple example to demonstrate and validate cross process behavior of the action queue.
//!
//! This binary has both parent and child execution flows. The parent process will spawn
//! N child processes which will either consume or produce actions.
//!
//! The output of the child processes is captured and printed at the end of the parent
//! execution.
//!
//! # Examples
//!
//! ## Produce from Parent, consume in children
//! ```skip
//! cargo run --example cross_process -- primary <CHILD_COUNT> <ACTION_COUNT>
//! ```
//!
//! ## Produce from children, consume in parent.
//!
//! ```skip
//! cargo run --example cross_process -- primary <CHILD_COUNT> <ACTION_COUNT> true
//! ```
//!
#![allow(clippy::print_stdout)]

use clap::{Parser, Subcommand};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Handler, NoopError, Type, WriterGuard,
};
use proton_action_queue::queue::{
    NoopOnlineStatusWaiterBuilder, Queue, QueueAutoExecutorPool, TokioTaskSpawner,
};
use proton_action_queue::rebase::RebaseChangeSet;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, StashConfiguration};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Subcommand)]
enum Commands {
    Primary {
        action_count: usize,
        process_count: usize,
        consume: Option<bool>,
    },
    Consumer {
        db_path: PathBuf,
    },
    Producer {
        db_path: PathBuf,
        action_count: usize,
    },
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Primary {
            action_count,
            process_count,
            consume,
        } => parent_main(process_count, action_count, consume.unwrap_or(false)).await,
        Commands::Consumer { db_path } => {
            child_main(&db_path, None).await;
        }
        Commands::Producer {
            db_path,
            action_count,
        } => {
            child_main(&db_path, Some(action_count)).await;
        }
    }
}

async fn parent_main(process_count: usize, action_count: usize, consume: bool) {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let queue = new_queue(tmp_dir.path()).await;

    println!("PRIMARY_ID= {}", executor_id());
    println!("STARTING CHILD PROCESS COUNT={process_count}");
    // create n processes
    let child_handles = std::iter::repeat_n((), process_count)
        .map(|()| {
            let tmp_dir = tmp_dir.path().to_path_buf();
            spawn_process(tmp_dir, if consume { Some(action_count) } else { None })
        })
        .collect::<Vec<_>>();

    assert_eq!(child_handles.len(), process_count);

    if consume {
        // In consume mode the parent process executes items.
        println!("CONSUMING ACTIONS");
        // due to lack of cross process observability we need loop until the first
        // action comes in.
        let executor = queue.new_executor();
        while executor.execute_one().await.unwrap().is_none() {}
        drop(executor);
        // we can now auto execute from here on forward.
        let task_spawner = TokioTaskSpawner;
        let online = NoopOnlineStatusWaiterBuilder;
        let _executors = QueueAutoExecutorPool::new(
            &queue,
            &ActionGroup::default(),
            NonZeroUsize::new(process_count * 2).unwrap(),
            &online,
            false,
            &task_spawner,
            tracing::Span::current(),
        );
        wait_on_queue_empty(&queue).await;
    } else {
        // Otherwise we produce actions and the children execute them.
        println!("QUEUEING_ACTIONS");
        for _ in 0..action_count {
            queue.queue_action(TestAction).await.unwrap();
        }

        wait_on_queue_empty(&queue).await;
    }

    for (stdin, handle) in child_handles {
        drop(stdin);
        handle.join().unwrap();
    }
    println!("DONE");
}

async fn wait_on_queue_empty(queue: &Queue) {
    loop {
        let remaining = queue.queued_actions_count().await.unwrap();
        println!("REMAINING {remaining}");
        if remaining == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
async fn child_main(directory: &Path, action_count: Option<usize>) {
    let queue = new_queue(directory).await;
    let notifier = Arc::new(tokio::sync::Notify::new());
    let notifier_cloned = notifier.clone();
    tokio::task::spawn_blocking(move || {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        notifier_cloned.notify_one();
    });
    println!("STARTING CHILD: {}", executor_id());
    if let Some(action_count) = action_count {
        for _ in 0..action_count {
            queue.queue_action(TestAction).await.unwrap();
        }
    } else {
        let task_spawner = TokioTaskSpawner;
        let online = NoopOnlineStatusWaiterBuilder;
        let _executors = QueueAutoExecutorPool::new(
            &queue,
            &ActionGroup::default(),
            NonZeroUsize::new(2).unwrap(),
            &online,
            false,
            &task_spawner,
            tracing::Span::current(),
        );
        notifier.notified().await;
    }
    println!("STOPPING CHILD: {}", executor_id());
}

fn spawn_process(
    path: PathBuf,
    action_count: Option<usize>,
) -> (std::process::ChildStdin, std::thread::JoinHandle<()>) {
    println!("SPAWNING CHILD {}", path.display());
    let this_process = std::env::args().take(1).next().unwrap();
    let in_pipe = std::process::Stdio::piped();
    let output = std::process::Stdio::piped();
    let mut command = Command::new(this_process);
    if let Some(action_count) = action_count {
        command
            .arg("producer")
            .arg(path)
            .arg(action_count.to_string());
    } else {
        command.arg("consumer").arg(path);
    }
    command.stdin(in_pipe).stdout(output);

    let mut child = command.spawn().unwrap();
    let stdin = child.stdin.take().unwrap();
    let handle = std::thread::spawn(move || {
        let output = child.wait_with_output().unwrap();
        println!("{}", String::from_utf8(output.stdout).unwrap());
    });
    (stdin, handle)
}

async fn new_queue(directory: &Path) -> Queue {
    let stash = stash::stash::Stash::new(StashConfiguration::test_with_path(
        &directory.join("sqlite.db"),
    ))
    .unwrap();

    let queue = Queue::new(stash).await.unwrap();

    queue.register::<TestAction>(TestHandler).unwrap();
    queue
}

#[derive(Debug, Serialize, Deserialize)]
struct TestAction;

impl Action for TestAction {
    const TYPE: Type = Type("test_action");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<TestAction>;
    type Handler = TestHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = NoopError;
}

#[derive(Default)]
struct TestHandler;

impl Handler for TestHandler {
    type Action = TestAction;

    async fn apply_local(
        &self,
        id: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        println!(
            "Executor [{}] - Action [{}] - Apply local",
            executor_id(),
            id
        );
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        id: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!(
            "Executor [{}] - Action [{}] - Apply Remote",
            executor_id(),
            id
        );
        Ok(())
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}

static EXECUTOR_ID: OnceLock<String> = OnceLock::new();

fn executor_id() -> &'static str {
    EXECUTOR_ID
        .get_or_init(|| Uuid::new_v4().to_string())
        .as_ref()
}
