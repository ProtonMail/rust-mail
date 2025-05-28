#![allow(clippy::print_stdout)]

use crate::app::App;
use crate::cli::Cli;
use tao::event_loop::EventLoopBuilder;
use tracing_subscriber::EnvFilter;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate tracing;

mod app;
mod cli;
mod keychain;
mod notifier;

type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    let events = EventLoopBuilder::with_user_event().build();

    tokio::spawn(Cli::run(events.create_proxy()));

    App::new(&events)?.run(events)
}
