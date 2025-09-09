//! Example: Modifying Mobile Settings
//!
//! This example demonstrates how to update mobile toolbar settings (actions) in Proton Mail.
//! Mobile settings control which actions are available in the mobile app toolbars for:
//! - List view (when viewing a list of conversations/messages)
//! - Message view (when viewing individual messages)
//! - Conversation view (when viewing conversations)
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example put_mobile_actions -- \
//!   --username your_username@protonmail.com \
//!   --password your_password \
//!   --toolbar-type list
//! ```
//!
//! Available toolbar types:
//! - `list`: Updates list toolbar actions (ToggleRead, ToggleStar, Archive, Trash, Move)
//! - `message`: Updates message toolbar actions (Reply, Forward, Print, ToggleStar)
//! - `conversation`: Updates conversation toolbar actions (ToggleRead, Archive, Label, Snooze)
//!
//! The example will:
//! 1. Login to your Proton Mail account
//! 2. Display current mobile settings
//! 3. Update the specified toolbar with predefined actions
//! 4. Wait for the API request to complete
//! 5. Display the updated settings

use std::sync::Arc;

use clap::Parser;
use proton_action_queue::observers::ActionAwaiter;
use proton_action_queue::queue::BroadcastMessage;
use proton_core_common::Origin;
use proton_core_common::datatypes::{ApiConfig, AppDetails};
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_log_service::LogService;
use proton_mail_common::MailContext;
use proton_mail_common::actions::mail_settings::{ToolbarType, UpdateMobileActions};
use proton_mail_common::datatypes::MobileAction;
use proton_mail_common::models::MailSettings;
use tempfile::TempDir;
use tokio::runtime;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

/// Example program demonstrating how to modify mobile settings (toolbar actions) via the Proton Mail API
///
/// This example logs into a Proton Mail account and updates mobile toolbar settings.
/// You can specify which toolbar to update: list, message, or conversation.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Proton Mail username
    #[arg(short, long)]
    username: String,

    /// Proton Mail password
    #[arg(short, long)]
    password: String,

    /// Type of toolbar to update: list, message, or conversation
    #[clap(short, long, default_value = "list")]
    toolbar_type: String,
}
#[tokio::main]
async fn main() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .parse_lossy(
            "info,proton_sqlite3=trace,\
                proton_core_common=trace,proton_mail_common=trace,\
                proton_event_loop=trace,proton_core_api=trace,\
                proton_action_queue=trace,proton_mail_api=trace,\
                stash=error",
        );
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .init();

    let Args {
        username,
        password,
        toolbar_type,
    } = Args::parse();
    let tmp_dir = TempDir::new().unwrap();

    let keychain = InMemoryKeyChain::default();
    let key = SessionEncryptionKey::random();
    keychain.store(key).unwrap();

    info!("TMP DIR: {:?}", tmp_dir.path());

    let config = proton_log_service::Config::builder()
        .name("log".into())
        .directory(tmp_dir.path().into())
        .build();
    let api_config = ApiConfig {
        app_details: AppDetails {
            platform: "ios".into(),
            product: "mail".into(),
            version: "7.1.0".into(),
        },
        ..Default::default()
    };

    let ctx = MailContext::new(
        Origin::App,
        runtime::Handle::current(),
        tmp_dir.path().join("session"),
        tmp_dir.path().join("user"),
        tmp_dir.path().join("core_cache"),
        tmp_dir.path().join("mail_cache"),
        50 * 1204 * 1024,
        Arc::new(keychain),
        api_config,
        None,
        None,
        LogService::new(config),
        EventPollMode::Manual,
        Default::default(),
    )
    .await
    .unwrap();

    let mut flow = ctx.new_login_flow().await.unwrap();

    flow.login_with_credentials(username, password, None)
        .await
        .unwrap();

    let user_ctx = ctx.user_context_from_login_flow(&mut flow).await.unwrap();

    info!("Successfully logged in. Now modifying mobile settings...");

    // Get current mail settings to see the current state
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let current_settings = MailSettings::get_or_default(&tether).await;

    info!(
        "Current mobile settings: {:?}",
        current_settings.mobile_settings
    );

    // Define some example actions for different toolbars
    // List toolbar: Actions available when viewing a list of emails/conversations
    let list_actions = vec![
        MobileAction::ToggleRead, // Mark conversation as read/unread
        MobileAction::ToggleStar, // Star/unstar
        MobileAction::Label,      // Add labels to conversation
        MobileAction::Move,       // Move to folder
    ];

    // Message toolbar: Actions available when viewing individual messages
    let message_actions = vec![
        MobileAction::ToggleRead, // Mark as read/unread
        MobileAction::Reply,      // Reply to message
        MobileAction::Forward,    // Forward message
    ];

    // Conversation toolbar: Actions available when viewing conversations
    let conversation_actions = vec![
        MobileAction::ToggleRead, // Mark conversation as read/unread
        MobileAction::ToggleStar, // Star/unstar
        MobileAction::Label,      // Add labels to conversation
        MobileAction::Move,       // Move to folder
    ];

    // Update mobile settings based on the toolbar type specified
    let (actions, toolbar_type_enum) = match toolbar_type.as_str() {
        "list" => {
            info!("Updating list toolbar actions: {:?}", list_actions);
            (list_actions, ToolbarType::List)
        }
        "message" => {
            info!("Updating message toolbar actions: {:?}", message_actions);
            (message_actions, ToolbarType::Message)
        }
        "conversation" => {
            info!(
                "Updating conversation toolbar actions: {:?}",
                conversation_actions
            );
            (conversation_actions, ToolbarType::Conversation)
        }
        _ => {
            error!(
                "Invalid toolbar type: {}. Valid options are: list, message, conversation",
                toolbar_type
            );
            return;
        }
    };

    // Create and queue the action
    let action = UpdateMobileActions::new(toolbar_type_enum, actions, false).unwrap();
    let queued_action = user_ctx.action_queue().queue_action(action).await.unwrap();
    let action_id = queued_action.id;

    info!(
        "Mobile settings update action queued with ID: {:?}",
        action_id
    );

    // Wait for the action to complete
    let awaiter = ActionAwaiter::new(user_ctx.action_queue(), action_id);

    match awaiter.wait().await.unwrap() {
        BroadcastMessage::Success(_) => {
            info!("Mobile settings successfully updated!");

            // Get the updated settings to confirm the change
            let updated_settings = MailSettings::get_or_default(&tether).await;
            info!(
                "Updated mobile settings: {:?}",
                updated_settings.mobile_settings
            );
        }
        BroadcastMessage::Error(err, _) => {
            error!("Error updating mobile settings: {:?}", err);
        }
        BroadcastMessage::Cancelled(_) => {
            info!("Mobile settings update was cancelled.");
        }
        BroadcastMessage::Deleted(_, _) => {
            info!("Mobile settings update action was deleted.");
        }
    }
}
