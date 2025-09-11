use insta::assert_snapshot;
use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;
use proton_mail_common::datatypes::exclusive_location::ExclusiveLocation;
use proton_mail_common::datatypes::{Disposition, SystemLabelId as _};
use proton_mail_common::models::attachment_cache::AttachmentCacheMetadata;
use proton_mail_common::models::{Attachment, Conversation};
use proton_mail_common::test_utils::utils::create_address;
use stash::orm::Model;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use proton_mail_common::models::{AttachmentType, Message};

use proton_mail_common::MailContextError;
use proton_mail_common::test_utils::test_context::MailTestContext;
use std::sync::atomic::AtomicU64;
// FIXME: There is duplicated logic from mail/mail-common/src/models/attachment_cache.rs
// This will be removed when we delete the mail-test-utils crate.

#[derive(Default)]
struct UsedVariables {
    /// Unique identifier that will get used to identify the item. It will act as the path.
    attachment_name: &'static str,
    disposition: Disposition,
    hit_count: u64,
    att_type: AttachmentType,
    spam: bool,
    trash: bool,
    starred: bool,
    /// Last time it was accessed
    atime: Duration,
    size: u64,
}
impl UsedVariables {
    fn unpack(self, now: Duration) -> (AttachmentCacheMetadata, Attachment, Message) {
        let at_cache = AttachmentCacheMetadata {
            attachment_id: 0.into(),
            atime: (now - self.atime).as_secs(),
            hit_count: self.hit_count,
            size: self.size,
            path: String::from(self.attachment_name),
            ctime: Default::default(),
        };

        let attachment = Attachment {
            disposition: self.disposition,
            attachment_type: self.att_type,
            filename: self.attachment_name.into(),
            ..Default::default()
        };

        let mut message = Message::test_default();

        if self.starred {
            message.label_ids.push(LabelId::starred());
        }
        if self.trash {
            message.exclusive_location = Some(ExclusiveLocation::System {
                name: SystemLabel::Trash,
                local_id: 0.into(),
            });
        }
        if self.spam {
            if self.trash {
                panic!("Conflicting trash and spam fields")
            }

            message.exclusive_location = Some(ExclusiveLocation::System {
                name: SystemLabel::Trash,
                local_id: 1.into(),
            });
        }
        (at_cache, attachment, message)
    }
}

fn default() -> UsedVariables {
    static UNIQUE: AtomicU64 = AtomicU64::new(0);
    let remote_id = UNIQUE.fetch_add(1, Ordering::Relaxed).to_string();

    UsedVariables {
        att_type: AttachmentType::Remote(Some(remote_id.into())),
        ..Default::default()
    }
}
fn kb(bytes: u64) -> u64 {
    bytes * 1024
}

fn days(days: u64) -> Duration {
    Duration::from_secs(60 * 60 * 24 * days)
}
fn comprehensive() -> impl Iterator<Item = UsedVariables> {
    [
        // Zero-sized attachments should always be preserved regardless of other factors
        UsedVariables {
            attachment_name: "zero_sized_in_trash",
            size: 0,
            trash: true,
            atime: days(365),
            hit_count: 0,
            ..default()
        },
        // Spam and trash have catastrophic impact on priority
        UsedVariables {
            attachment_name: "spam_tiny_new",
            size: kb(5),
            spam: true,
            atime: Duration::from_secs(3600), // 1 hour
            hit_count: 3,
            ..default()
        },
        UsedVariables {
            attachment_name: "trash_tiny_new",
            size: kb(5),
            trash: true,
            atime: Duration::from_secs(3600), // 1 hour
            hit_count: 3,
            ..default()
        },
        // Age significantly impacts priority
        UsedVariables {
            attachment_name: "ancient_tiny",
            size: kb(10),
            atime: days(720), // 2 years
            hit_count: 1,
            ..default()
        },
        UsedVariables {
            attachment_name: "recent_large",
            size: kb(200),
            atime: Duration::from_secs(3600 * 24), // 1 day
            hit_count: 1,
            ..default()
        },
        // Hit count provides significant protection
        UsedVariables {
            attachment_name: "frequently_accessed_medium",
            size: kb(50),
            hit_count: 20,
            atime: days(30),
            ..default()
        },
        UsedVariables {
            attachment_name: "rarely_accessed_small",
            size: kb(20),
            hit_count: 0,
            atime: days(30),
            ..default()
        },
        // Starred messages get significant protection
        UsedVariables {
            attachment_name: "starred_large",
            size: kb(100),
            starred: true,
            hit_count: 1,
            atime: days(14),
            ..default()
        },
        UsedVariables {
            attachment_name: "unstarred_medium",
            size: kb(40),
            starred: false,
            hit_count: 1,
            atime: days(14),
            ..default()
        },
        // PGP attachments are prioritized
        UsedVariables {
            attachment_name: "pgp_large",
            size: kb(80),
            hit_count: 1,
            atime: days(7),
            ..default()
        },
        UsedVariables {
            attachment_name: "normal_small",
            size: kb(30),
            hit_count: 1,
            atime: days(7),
            ..default()
        },
        // Disposition differences
        UsedVariables {
            attachment_name: "inline_medium",
            size: kb(60),
            disposition: Disposition::Inline,
            hit_count: 1,
            atime: days(5),
            ..default()
        },
        UsedVariables {
            attachment_name: "attachment_medium",
            size: kb(60),
            disposition: Disposition::Attachment,
            hit_count: 1,
            atime: days(5),
            ..default()
        },
        // Complex combinations
        UsedVariables {
            attachment_name: "starred_trash_large", // competing factors
            size: kb(90),
            starred: true,
            trash: true,
            hit_count: 2,
            atime: days(2),
            ..default()
        },
        UsedVariables {
            attachment_name: "pgp_old_but_frequent", // competing factors
            size: kb(70),
            hit_count: 15,
            atime: days(180),
            ..default()
        },
        UsedVariables {
            attachment_name: "tiny_inline_ancient_but_starred", // competing factors
            size: kb(5),
            disposition: Disposition::Inline,
            atime: days(500),
            starred: true,
            hit_count: 1,
            ..default()
        },
        // Additional edge cases
        UsedVariables {
            attachment_name: "unsent_draft_large",
            size: kb(150),
            atime: days(60),
            hit_count: 0,
            ..default()
        },
        UsedVariables {
            attachment_name: "fresh_but_huge",
            size: kb(500),
            atime: Duration::from_secs(60), // 1 minute
            hit_count: 0,
            ..default()
        },
        UsedVariables {
            attachment_name: "perfect_storm", // everything negative
            size: kb(300),
            trash: true,
            atime: days(365),
            hit_count: 0,
            disposition: Disposition::Attachment,
            ..default()
        },
        UsedVariables {
            attachment_name: "golden_child", // everything positive
            size: kb(30),
            starred: true,
            disposition: Disposition::Inline,
            hit_count: 10,
            atime: Duration::from_secs(300), // 5 minutes
            ..default()
        },
        UsedVariables {
            attachment_name: "name/foo.png",
            size: kb(30),
            disposition: Disposition::Attachment,
            hit_count: 10,
            atime: Duration::from_secs(300), // 5 minutes
            ..default()
        },
    ]
    .into_iter()
}

#[tokio::test]
async fn integration() -> anyhow::Result<()> {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = &mut user_ctx.user_stash().connection().await?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards");

    let mut conv = Conversation {
        remote_id: Some("1".into()),
        ..Conversation::test_default()
    };
    let addr = create_address(tether).await;
    tether.tx(async |tx| conv.save(tx).await).await?;

    // Evil hack to stop cleanup
    user_ctx
        .attachment_cache_state()
        .is_cleanup_running()
        .store(true, Ordering::SeqCst);

    tether
        .tx::<_, _, MailContextError>(async |tx| {
            for var in comprehensive() {
                let (mut at_cache, mut at, mut msg) = var.unpack(now);
                msg.local_conversation_id = conv.local_id;
                msg.remote_conversation_id = conv.remote_id.clone();
                msg.local_address_id = addr.id();
                msg.remote_address_id = addr.remote_id.clone().unwrap();
                msg.save(tx).await?;

                at.local_message_id = msg.local_id;
                at.local_address_id = addr.local_id;
                at.remote_address_id = addr.remote_id.clone();
                at.save(tx).await?;

                // First request the data
                let path = Attachment::store_in_cache(
                    &user_ctx,
                    &at.filename,
                    at.id(),
                    vec![0; at_cache.size.try_into().unwrap()],
                    tx,
                )
                .await?;

                // Then override locally ;)
                at_cache.attachment_id = at.id();
                at_cache.path = path.into_os_string().into_string().unwrap();
                at_cache.save(tx).await?;
            }

            Ok(())
        })
        .await?;

    let files_before = tether
        .query_values::<_, String>("SELECT path FROM attachment_cache", vec![])
        .await?
        .into_iter()
        .map(PathBuf::from)
        .map(|x| x.file_name().unwrap().to_str().unwrap().to_owned())
        .join("\n");

    // Let's allow it to cleanup
    user_ctx
        .attachment_cache_state()
        .is_cleanup_running()
        .store(false, Ordering::SeqCst);

    Attachment::do_cleanup_cache(&user_ctx).await?;

    let files_after = tether
        .query_values::<_, String>("SELECT path FROM attachment_cache", vec![])
        .await?
        .into_iter()
        .map(PathBuf::from)
        .map(|x| x.file_name().unwrap().to_str().unwrap().to_owned())
        .join("\n");

    let output = format!(
        "
This test shows the before and after of a cleanup with a max size of {} bytes

---------- All attachments ----------
{files_before}


---------- Cleaned attachments ----------
{files_after}
",
        user_ctx.mail_context().attachment_cache_size,
    );

    assert_snapshot!(output);

    Ok(())
}
