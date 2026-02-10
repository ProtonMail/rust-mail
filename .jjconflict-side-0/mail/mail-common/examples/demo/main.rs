#![allow(clippy::print_stdout)]

use crate::app::events::Proxy;
use crate::cli::Cli;
use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[macro_use]
extern crate anyhow;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate tracing;

mod app;
mod cli;
mod keychain;
mod notifier;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    Cli::run(DummyProxy).await
}

#[derive(Clone)]
struct DummyProxy;

impl Proxy for DummyProxy {}
