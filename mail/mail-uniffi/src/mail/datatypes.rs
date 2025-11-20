mod attachment;
mod available_action;
mod folder_banner;
pub(crate) mod labels;
mod snooze;
mod system_folder;
mod system_label;

use crate::core::datatypes::{AvatarInformation, Id, UnixTimestamp};
use crate::errors::ActionError;
use crate::mail::MailUserSession;
pub use crate::{UniffiEnum, UniffiRecord};
pub use attachment::*;
pub use available_action::*;
use core::fmt;
pub use folder_banner::*;
use itertools::Itertools;
use parking_lot::Mutex;
use proton_core_common::datatypes::{
    AvatarInformation as RealAvatarInformation, LabelColor as RealLabelColor,
    LabelType as RealLabelType, LocalAddressId, LocalLabelId,
};
use proton_core_common::models::{Address as RealAddress, Label as RealLabel, ModelIdExtension};
use proton_core_common::utils::MapVec as _;
use proton_mail_api::MAX_PAGE_ELEMENT_COUNT_U64;
use proton_mail_api::services::proton::request_data::MessageMetadataSortMode as RealMessageMetadataSortMode;
use proton_mail_api::services::proton::requests::{GetConversationsOptions, GetMessagesOptions};
use proton_mail_common::ProtonMailError;
use proton_mail_common::actions::{LabelAsOutput as RealLabelAsOutput, Undo as RealUndo};
use proton_mail_common::datatypes::{
    AlmostAllMail as RealAlmostAllMail, AttachmentMetadata as RealAttachmentMetadata,
    ComposerDirection as RealComposerDirection, ComposerMode as RealComposerMode,
    CustomLabel as RealCustomLabel, Disposition as RealDisposition,
    LabelDescription as RealLabelDescription, LocalConversationId, LocalMessageId,
    MessageButtons as RealMessageButtons, MessageFlags as RealMessageFlags,
    MessageRecipient as RealMessageRecipient,
    MessageRecipientDisplayMode as RealMessageRecipientDisplayMode,
    MessageSender as RealMessageSender, MimeType as RealMimeType, MobileAction as RealMobileAction,
    MobileSetting as RealMobileSetting, MobileSettings as RealMobileSettings,
    NextMessageOnMove as RealNextMessageOnMove, ParsedHeaderValue as RealParsedHeaderValue,
    PgpScheme as RealPgpScheme, PmSignature as RealPmSignature, ShowImages as RealShowImages,
    ShowMoved as RealShowMoved, SpamAction as RealSpamAction, SwipeAction as RealSwipeAction,
    ViewLayout as RealViewLayout, ViewMode as RealViewMode,
};
use proton_mail_common::datatypes::{
    ContextualConversation, ExclusiveLocation as RealExclusiveLocation,
    HiddenMessagesBanner as RealHiddenMessagesBanner,
};
use proton_mail_common::draft::recipients::MaybeEmptyString;
use proton_mail_common::models::{
    Conversation as RealConversation, MailSettings as RealMailSettings, Message as RealMessage,
    MessageMimeType, MessageReplyTo as RealMessageReplyTo,
};
use smart_default::SmartDefault;
pub use snooze::*;
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::fmt::{Display, Formatter};
use std::sync::Arc;
pub use system_label::*;
use tracing::warn;
use uniffi_runtime::uniffi_async;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum AlmostAllMail {
    AllMail = 0,

    #[default]
    AlmostAllMail = 1,
}

impl From<AlmostAllMail> for RealAlmostAllMail {
    fn from(value: AlmostAllMail) -> Self {
        match value {
            AlmostAllMail::AllMail => RealAlmostAllMail::AllMail,
            AlmostAllMail::AlmostAllMail => RealAlmostAllMail::AlmostAllMail,
        }
    }
}

impl From<RealAlmostAllMail> for AlmostAllMail {
    fn from(value: RealAlmostAllMail) -> Self {
        match value {
            RealAlmostAllMail::AllMail => AlmostAllMail::AllMail,
            RealAlmostAllMail::AlmostAllMail => AlmostAllMail::AlmostAllMail,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ComposerDirection {
    #[default]
    LeftToRight = 0,
    RightToLeft = 1,
}

impl From<ComposerDirection> for RealComposerDirection {
    fn from(value: ComposerDirection) -> Self {
        match value {
            ComposerDirection::LeftToRight => RealComposerDirection::LeftToRight,
            ComposerDirection::RightToLeft => RealComposerDirection::RightToLeft,
        }
    }
}

impl From<RealComposerDirection> for ComposerDirection {
    fn from(value: RealComposerDirection) -> Self {
        match value {
            RealComposerDirection::LeftToRight => ComposerDirection::LeftToRight,
            RealComposerDirection::RightToLeft => ComposerDirection::RightToLeft,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ComposerMode {
    #[default]
    Normal = 0,
    Maximized = 1,
}

impl From<ComposerMode> for RealComposerMode {
    fn from(value: ComposerMode) -> Self {
        match value {
            ComposerMode::Normal => RealComposerMode::Normal,
            ComposerMode::Maximized => RealComposerMode::Maximized,
        }
    }
}

impl From<RealComposerMode> for ComposerMode {
    fn from(value: RealComposerMode) -> Self {
        match value {
            RealComposerMode::Normal => ComposerMode::Normal,
            RealComposerMode::Maximized => ComposerMode::Maximized,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum Disposition {
    Attachment = 1,
    Inline = 2,
}

impl From<Disposition> for RealDisposition {
    fn from(value: Disposition) -> Self {
        match value {
            Disposition::Attachment => RealDisposition::Attachment,
            Disposition::Inline => RealDisposition::Inline,
        }
    }
}

impl From<RealDisposition> for Disposition {
    fn from(value: RealDisposition) -> Self {
        match value {
            RealDisposition::Attachment => Disposition::Attachment,
            RealDisposition::Inline => Disposition::Inline,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum ExclusiveLocation {
    System {
        name: SystemLabel,
        id: Id,
    },
    Custom {
        name: String,
        id: Id,
        color: LabelColor,
    },
}

impl From<ExclusiveLocation> for RealExclusiveLocation {
    fn from(value: ExclusiveLocation) -> Self {
        match value {
            ExclusiveLocation::System { name, id } => RealExclusiveLocation::System {
                name: name.into(),
                local_id: id.into(),
            },
            ExclusiveLocation::Custom { name, id, color } => RealExclusiveLocation::Custom {
                name,
                local_id: id.into(),
                color: color.into(),
            },
        }
    }
}

impl From<RealExclusiveLocation> for ExclusiveLocation {
    fn from(value: RealExclusiveLocation) -> Self {
        match value {
            RealExclusiveLocation::System { name, local_id } => ExclusiveLocation::System {
                name: name.into(),
                id: local_id.into(),
            },
            RealExclusiveLocation::Custom {
                name,
                local_id,
                color,
            } => ExclusiveLocation::Custom {
                name,
                id: local_id.into(),
                color: color.into(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum LabelType {
    Label = 1,
    ContactGroup = 2,
    Folder = 3,
    System = 4,
}

impl Display for LabelType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Label => write!(f, "Label"),
            Self::ContactGroup => write!(f, "Contact Group"),
            Self::Folder => write!(f, "Folder"),
            Self::System => write!(f, "System"),
        }
    }
}

impl From<LabelType> for RealLabelType {
    fn from(value: LabelType) -> Self {
        match value {
            LabelType::Label => RealLabelType::Label,
            LabelType::ContactGroup => RealLabelType::ContactGroup,
            LabelType::Folder => RealLabelType::Folder,
            LabelType::System => RealLabelType::System,
        }
    }
}

impl From<RealLabelType> for LabelType {
    fn from(value: RealLabelType) -> Self {
        match value {
            RealLabelType::Label => LabelType::Label,
            RealLabelType::ContactGroup => LabelType::ContactGroup,
            RealLabelType::Folder => LabelType::Folder,
            RealLabelType::System => LabelType::System,
        }
    }
}

/// Extended version of [`LabelType`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum LabelDescription {
    Label,
    ContactGroup,
    Folder,
    System(Option<SystemLabel>),
}

impl From<RealLabelDescription> for LabelDescription {
    fn from(value: RealLabelDescription) -> Self {
        match value {
            RealLabelDescription::Label => LabelDescription::Label,
            RealLabelDescription::ContactGroup => LabelDescription::ContactGroup,
            RealLabelDescription::Folder => LabelDescription::Folder,
            RealLabelDescription::System(system_label) => {
                let system_label = system_label.map(SystemLabel::from);
                LabelDescription::System(system_label)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum MessageButtons {
    #[default]
    ReadFirst = 0,
    UnreadFirst = 1,
}

impl From<MessageButtons> for RealMessageButtons {
    fn from(value: MessageButtons) -> Self {
        match value {
            MessageButtons::ReadFirst => RealMessageButtons::ReadFirst,
            MessageButtons::UnreadFirst => RealMessageButtons::UnreadFirst,
        }
    }
}

impl From<RealMessageButtons> for MessageButtons {
    fn from(value: RealMessageButtons) -> Self {
        match value {
            RealMessageButtons::ReadFirst => MessageButtons::ReadFirst,
            RealMessageButtons::UnreadFirst => MessageButtons::UnreadFirst,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum MessageMetadataSortMode {
    Time,
    SnoozeTime,
    Size,
    ID,
}

impl From<MessageMetadataSortMode> for RealMessageMetadataSortMode {
    fn from(value: MessageMetadataSortMode) -> Self {
        match value {
            MessageMetadataSortMode::Time => RealMessageMetadataSortMode::Time,
            MessageMetadataSortMode::SnoozeTime => RealMessageMetadataSortMode::SnoozeTime,
            MessageMetadataSortMode::Size => RealMessageMetadataSortMode::Size,
            MessageMetadataSortMode::ID => RealMessageMetadataSortMode::ID,
        }
    }
}

impl From<RealMessageMetadataSortMode> for MessageMetadataSortMode {
    fn from(value: RealMessageMetadataSortMode) -> Self {
        match value {
            RealMessageMetadataSortMode::Time => MessageMetadataSortMode::Time,
            RealMessageMetadataSortMode::SnoozeTime => MessageMetadataSortMode::SnoozeTime,
            RealMessageMetadataSortMode::Size => MessageMetadataSortMode::Size,
            RealMessageMetadataSortMode::ID => MessageMetadataSortMode::ID,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum MimeType {
    ApplicationJson = 1,
    ApplicationPdf = 2,
    MessageRfc822 = 3,
    MultipartMixed = 4,
    MultipartRelated = 5,
    #[default]
    TextHtml = 6,
    TextPlain = 7,
}

impl From<MimeType> for RealMimeType {
    fn from(value: MimeType) -> Self {
        match value {
            MimeType::ApplicationJson => RealMimeType::ApplicationJson,
            MimeType::ApplicationPdf => RealMimeType::ApplicationPdf,
            MimeType::MessageRfc822 => RealMimeType::MessageRfc822,
            MimeType::MultipartMixed => RealMimeType::MultipartMixed,
            MimeType::MultipartRelated => RealMimeType::MultipartRelated,
            MimeType::TextHtml => RealMimeType::TextHtml,
            MimeType::TextPlain => RealMimeType::TextPlain,
        }
    }
}

impl From<RealMimeType> for MimeType {
    fn from(value: RealMimeType) -> Self {
        match value {
            RealMimeType::ApplicationJson => MimeType::ApplicationJson,
            RealMimeType::ApplicationPdf => MimeType::ApplicationPdf,
            RealMimeType::MessageRfc822 => MimeType::MessageRfc822,
            RealMimeType::MultipartMixed => MimeType::MultipartMixed,
            RealMimeType::MultipartRelated => MimeType::MultipartRelated,
            RealMimeType::TextHtml => MimeType::TextHtml,
            RealMimeType::TextPlain => MimeType::TextPlain,
        }
    }
}

impl From<MessageMimeType> for MimeType {
    fn from(value: MessageMimeType) -> Self {
        RealMimeType::from(value).into()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum NextMessageOnMove {
    #[default]
    DisabledExplicit = 0,
    DisabledImplicit = 1,
    EnabledExplicit = 2,
}

impl From<NextMessageOnMove> for RealNextMessageOnMove {
    fn from(value: NextMessageOnMove) -> Self {
        match value {
            NextMessageOnMove::DisabledExplicit => RealNextMessageOnMove::DisabledExplicit,
            NextMessageOnMove::DisabledImplicit => RealNextMessageOnMove::DisabledImplicit,
            NextMessageOnMove::EnabledExplicit => RealNextMessageOnMove::EnabledExplicit,
        }
    }
}

impl From<RealNextMessageOnMove> for NextMessageOnMove {
    fn from(value: RealNextMessageOnMove) -> Self {
        match value {
            RealNextMessageOnMove::DisabledExplicit => NextMessageOnMove::DisabledExplicit,
            RealNextMessageOnMove::DisabledImplicit => NextMessageOnMove::DisabledImplicit,
            RealNextMessageOnMove::EnabledExplicit => NextMessageOnMove::EnabledExplicit,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum ParsedHeaderValue {
    Array(Vec<String>),
    String(String),
}

impl From<ParsedHeaderValue> for RealParsedHeaderValue {
    fn from(value: ParsedHeaderValue) -> Self {
        match value {
            ParsedHeaderValue::Array(array) => RealParsedHeaderValue::Array(array),
            ParsedHeaderValue::String(string) => RealParsedHeaderValue::String(string),
        }
    }
}

impl From<RealParsedHeaderValue> for ParsedHeaderValue {
    fn from(value: RealParsedHeaderValue) -> Self {
        match value {
            RealParsedHeaderValue::Array(array) => ParsedHeaderValue::Array(array),
            RealParsedHeaderValue::String(string) => ParsedHeaderValue::String(string),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum PgpScheme {
    Inline = 8,
    #[default]
    Mime = 16,
}

impl From<PgpScheme> for RealPgpScheme {
    fn from(value: PgpScheme) -> Self {
        match value {
            PgpScheme::Inline => RealPgpScheme::Inline,
            PgpScheme::Mime => RealPgpScheme::Mime,
        }
    }
}

impl From<RealPgpScheme> for PgpScheme {
    fn from(value: RealPgpScheme) -> Self {
        match value {
            RealPgpScheme::Inline => PgpScheme::Inline,
            RealPgpScheme::Mime => PgpScheme::Mime,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, UniffiRecord)]
pub struct PmSignature {
    value: u8,
}

impl Default for PmSignature {
    fn default() -> Self {
        Self {
            value: RealPmSignature::ENABLED.bits(),
        }
    }
}

impl From<RealPmSignature> for PmSignature {
    fn from(value: RealPmSignature) -> Self {
        Self {
            value: value.bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ShowImages {
    DoNotAutoLoad = 0,
    AutoLoadRemote = 1,
    #[default]
    AutoLoadEmbedded = 2,
    AutoLoadBoth = 3,
}

impl From<ShowImages> for RealShowImages {
    fn from(value: ShowImages) -> Self {
        match value {
            ShowImages::DoNotAutoLoad => RealShowImages::DoNotAutoLoad,
            ShowImages::AutoLoadRemote => RealShowImages::AutoLoadRemote,
            ShowImages::AutoLoadEmbedded => RealShowImages::AutoLoadEmbedded,
            ShowImages::AutoLoadBoth => RealShowImages::AutoLoadBoth,
        }
    }
}

impl From<RealShowImages> for ShowImages {
    fn from(value: RealShowImages) -> Self {
        match value {
            RealShowImages::DoNotAutoLoad => ShowImages::DoNotAutoLoad,
            RealShowImages::AutoLoadRemote => ShowImages::AutoLoadRemote,
            RealShowImages::AutoLoadEmbedded => ShowImages::AutoLoadEmbedded,
            RealShowImages::AutoLoadBoth => ShowImages::AutoLoadBoth,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ShowMoved {
    #[default]
    DoNotKeep = 0,
    KeepInDrafts = 1,
    KeepInSent = 2,
    KeepBoth = 3,
}

impl From<ShowMoved> for RealShowMoved {
    fn from(value: ShowMoved) -> Self {
        match value {
            ShowMoved::DoNotKeep => RealShowMoved::DoNotKeep,
            ShowMoved::KeepInDrafts => RealShowMoved::KeepInDrafts,
            ShowMoved::KeepInSent => RealShowMoved::KeepInSent,
            ShowMoved::KeepBoth => RealShowMoved::KeepBoth,
        }
    }
}

impl From<RealShowMoved> for ShowMoved {
    fn from(value: RealShowMoved) -> Self {
        match value {
            RealShowMoved::DoNotKeep => ShowMoved::DoNotKeep,
            RealShowMoved::KeepInDrafts => ShowMoved::KeepInDrafts,
            RealShowMoved::KeepInSent => ShowMoved::KeepInSent,
            RealShowMoved::KeepBoth => ShowMoved::KeepBoth,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum SpamAction {
    DoNothing = 0,
    UnsubscribeWithOneClick = 1,
}

impl From<SpamAction> for RealSpamAction {
    fn from(value: SpamAction) -> Self {
        match value {
            SpamAction::DoNothing => RealSpamAction::DoNothing,
            SpamAction::UnsubscribeWithOneClick => RealSpamAction::UnsubscribeWithOneClick,
        }
    }
}

impl From<RealSpamAction> for SpamAction {
    fn from(value: RealSpamAction) -> Self {
        match value {
            RealSpamAction::DoNothing => SpamAction::DoNothing,
            RealSpamAction::UnsubscribeWithOneClick => SpamAction::UnsubscribeWithOneClick,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
pub enum SwipeAction {
    NoAction,
    Trash,
    Spam,
    Star,
    #[default]
    Archive,
    MarkAsRead,
    LabelAs,
    MoveTo,
}

impl From<SwipeAction> for RealSwipeAction {
    fn from(value: SwipeAction) -> Self {
        match value {
            SwipeAction::NoAction => RealSwipeAction::NoAction,
            SwipeAction::Trash => RealSwipeAction::Trash,
            SwipeAction::Spam => RealSwipeAction::Spam,
            SwipeAction::Star => RealSwipeAction::Star,
            SwipeAction::Archive => RealSwipeAction::Archive,
            SwipeAction::MarkAsRead => RealSwipeAction::MarkAsRead,
            SwipeAction::LabelAs => RealSwipeAction::LabelAs,
            SwipeAction::MoveTo => RealSwipeAction::MoveTo,
        }
    }
}

impl From<RealSwipeAction> for SwipeAction {
    fn from(value: RealSwipeAction) -> Self {
        match value {
            RealSwipeAction::NoAction => SwipeAction::NoAction,
            RealSwipeAction::Trash => SwipeAction::Trash,
            RealSwipeAction::Spam => SwipeAction::Spam,
            RealSwipeAction::Star => SwipeAction::Star,
            RealSwipeAction::Archive => SwipeAction::Archive,
            RealSwipeAction::MarkAsRead => SwipeAction::MarkAsRead,
            RealSwipeAction::LabelAs => SwipeAction::LabelAs,
            RealSwipeAction::MoveTo => SwipeAction::MoveTo,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ViewLayout {
    #[default]
    Column = 0,
    Row = 1,
}

impl From<ViewLayout> for RealViewLayout {
    fn from(value: ViewLayout) -> Self {
        match value {
            ViewLayout::Column => RealViewLayout::Column,
            ViewLayout::Row => RealViewLayout::Row,
        }
    }
}

impl From<RealViewLayout> for ViewLayout {
    fn from(value: RealViewLayout) -> Self {
        match value {
            RealViewLayout::Column => ViewLayout::Column,
            RealViewLayout::Row => ViewLayout::Row,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ViewMode {
    #[default]
    Conversations = 0,
    Messages = 1,
}

impl From<ViewMode> for RealViewMode {
    fn from(value: ViewMode) -> Self {
        match value {
            ViewMode::Conversations => RealViewMode::Conversations,
            ViewMode::Messages => RealViewMode::Messages,
        }
    }
}

impl From<RealViewMode> for ViewMode {
    fn from(value: RealViewMode) -> Self {
        match value {
            RealViewMode::Conversations => ViewMode::Conversations,
            RealViewMode::Messages => ViewMode::Messages,
        }
    }
}

#[derive(UniffiEnum)]
pub enum MessageRecipientDisplayMode {
    Recipients,
    Sender,
}

impl From<RealMessageRecipientDisplayMode> for MessageRecipientDisplayMode {
    fn from(value: RealMessageRecipientDisplayMode) -> Self {
        match value {
            RealMessageRecipientDisplayMode::Recipients => Self::Recipients,
            RealMessageRecipientDisplayMode::Sender => Self::Sender,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AttachmentMetadata {
    pub id: Id,
    pub disposition: Disposition,
    pub mime_type: AttachmentMimeType,
    pub name: String,
    pub size: u64,
    pub is_listable: bool,
}

impl From<RealAttachmentMetadata> for AttachmentMetadata {
    fn from(value: RealAttachmentMetadata) -> Self {
        let is_listable = value.is_listable();

        AttachmentMetadata {
            // FIXME: This will exist after the cache MR is merged
            id: value.local_id.unwrap_or(u64::MAX.into()).into(),
            disposition: value.disposition.into(),
            mime_type: value.mime_type.into(),
            name: value.filename,
            size: value.size,
            is_listable,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Conversation {
    pub id: Id,
    pub attachments_metadata: Vec<AttachmentMetadata>,
    pub custom_labels: Vec<InlineCustomLabel>,
    pub display_snooze_reminder: bool,
    pub snoozed_until: Option<UnixTimestamp>,

    pub locations: Vec<ExclusiveLocation>,

    pub expiration_time: UnixTimestamp,
    pub is_starred: bool,
    pub num_attachments: u64,
    pub num_messages: u64,
    pub num_unread: u64,
    pub total_messages: u64,
    pub total_unread: u64,
    pub display_order: u64,
    pub recipients: Vec<MessageRecipient>,
    pub senders: Vec<MessageSender>,
    pub size: u64,
    pub subject: String,
    pub time: UnixTimestamp,
    pub avatar: AvatarInformation,
    pub hidden_messages_banner: Option<HiddenMessagesBanner>,
}

impl From<ContextualConversation> for Conversation {
    fn from(value: ContextualConversation) -> Self {
        let avatar = RealMessageSender::avatar_info(&value.senders.value);

        Self {
            id: value.local_id.into(),
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(Into::into)
                .collect(),
            custom_labels: value.custom_labels.map_vec(),
            display_order: value.display_order,
            display_snooze_reminder: value.display_snooze_reminder,
            locations: value.locations.into_iter().map(Into::into).collect(),
            expiration_time: value.expiration_time.into(),
            num_attachments: value.num_attachments,
            num_messages: value.num_messages,
            num_unread: value.num_unread,
            total_unread: value.total_unread,
            total_messages: value.total_messages,
            recipients: value
                .recipients
                .value
                .into_iter()
                .map(MessageRecipient::from)
                .collect(),
            senders: value
                .senders
                .value
                .into_iter()
                .map(MessageSender::from)
                .collect(),
            size: value.size,
            is_starred: value.is_starred,
            subject: value.subject,
            time: if value.display_snooze_reminder {
                value.snooze_time.into()
            } else {
                value.time.into()
            },
            snoozed_until: value.snoozed_until.map(Into::into),
            avatar: avatar.into(),
            hidden_messages_banner: value.hidden_messages_banner.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum HiddenMessagesBanner {
    ContainsTrashedMessages,
    ContainsNonTrashedMessages,
}

impl From<RealHiddenMessagesBanner> for HiddenMessagesBanner {
    fn from(value: RealHiddenMessagesBanner) -> Self {
        match value {
            RealHiddenMessagesBanner::ContainsTrashedMessages => {
                HiddenMessagesBanner::ContainsTrashedMessages
            }
            RealHiddenMessagesBanner::ContainsNonTrashedMessages => {
                HiddenMessagesBanner::ContainsNonTrashedMessages
            }
        }
    }
}

/// Parameters to filter/search conversations with a given criteria.
#[derive(Clone, Debug, SmartDefault, UniffiRecord)]
pub struct ConversationSearchOptions {
    /// Address ID to filter on.
    pub address_id: Option<Id>,

    /// If `true`, only return conversations which have attachments. If `false`,
    /// only return conversations which have no attachments.
    pub attachments: Option<bool>,

    /// If `true`, automatically convert simple queries to wildcarded versions,
    /// such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,

    /// UNIX timestamp to filter conversations earlier than timestamp.
    pub begin: Option<UnixTimestamp>,

    /// Return only conversations newer, in creation time (NOT timestamp), than
    /// the specified conversation ID if timestamp = `begin`.
    // TODO: Improve the documentation above, as it doesn't make total sense.
    pub begin_id: Option<Id>,

    /// If `true`, return results in descending order rather than ascending.
    pub desc: Option<bool>,

    /// UNIX timestamp to filter conversations later than timestamp.
    pub end: Option<UnixTimestamp>,

    /// Return only conversations older, in creation time (NOT timestamp), than
    /// the specified conversation ID if timestamp = `end`.
    // TODO: Improve the documentation above, as it doesn't make total sense.
    pub end_id: Option<Id>,

    /// Return only conversations with the specified anchor.
    pub anchor: Option<UnixTimestamp>,

    /// Return only conversations with the specified anchor ID.
    pub anchor_id: Option<Id>,

    /// Filter on external ID.
    // TODO: Document this properly.
    pub external_id: Option<String>,

    /// Keyword search of `From` field.
    pub from: Option<String>,

    /// Conversation IDs to filter on.
    pub ids: Option<Vec<Id>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label ID to filter on.
    pub label_id: Option<Id>,

    /// The number of conversations to return.
    pub limit: Option<u64>,

    /// Page index.
    pub page: u64,

    /// Number of elements per page.
    #[default(MAX_PAGE_ELEMENT_COUNT_U64)]
    pub page_size: u64,

    /// Keyword search of `To`, `CC`, and `BCC` fields.
    pub recipients: Option<Vec<String>>,

    /// Sort the results by one of the sorting modes.
    pub sort: Option<MessageMetadataSortMode>,

    /// Keyword search of `Subject` field.
    pub subject: Option<String>,

    /// If `true`, only return conversations which have unread messages. If
    /// `false`, only return conversations which have all messages read.
    pub unread: Option<bool>,
}

impl ConversationSearchOptions {
    pub async fn into_api_options(
        self,
        tether: &Tether,
    ) -> Result<GetConversationsOptions, StashError> {
        let ids = match self.ids {
            Some(local_ids) => {
                let mut ids = Vec::with_capacity(local_ids.len());
                for id in &local_ids {
                    if let Some(resolved_id) = RealConversation::local_id_counterpart(
                        LocalConversationId::from(*id),
                        tether,
                    )
                    .await?
                    {
                        ids.push(resolved_id);
                    }
                }
                if ids.is_empty() { None } else { Some(ids) }
            }
            None => None,
        };

        Ok(GetConversationsOptions {
            address_id: match self.address_id {
                Some(id) => {
                    RealAddress::local_id_counterpart(LocalAddressId::from(id), tether).await?
                }
                None => None,
            },
            attachments: self.attachments,
            auto_wildcard: self.auto_wildcard,
            begin: self.begin.map(|v| v.0),
            begin_id: match self.begin_id {
                Some(id) => {
                    RealConversation::local_id_counterpart(LocalConversationId::from(id), tether)
                        .await?
                }
                None => None,
            },
            desc: self.desc,
            end: self.end.map(|v| v.0),
            end_id: match self.end_id {
                Some(id) => {
                    RealConversation::local_id_counterpart(LocalConversationId::from(id), tether)
                        .await?
                }
                None => None,
            },
            anchor: self.anchor.map(|v| v.0),
            anchor_id: match self.anchor_id {
                Some(id) => {
                    RealConversation::local_id_counterpart(LocalConversationId::from(id), tether)
                        .await?
                }
                None => None,
            },
            external_id: self.external_id,
            from: self.from,
            ids,
            keyword: self.keyword,
            label_id: match self.label_id {
                Some(id) => RealLabel::local_id_counterpart(LocalLabelId::from(id), tether).await?,
                None => None,
            },
            limit: self.limit,
            page: self.page,
            page_size: self.page_size,
            recipients: self.recipients,
            sort: self.sort.map(Into::into),
            subject: self.subject,
            unread: self.unread,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, UniffiRecord)]
pub struct InlineCustomLabel {
    pub id: Id,
    pub name: String,
    pub color: LabelColor,
}

impl From<RealCustomLabel> for InlineCustomLabel {
    fn from(value: RealCustomLabel) -> Self {
        Self {
            id: value.local_id.into(),
            name: value.name,
            color: value.color.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct LabelColor {
    value: String,
}

impl Display for LabelColor {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<LabelColor> for RealLabelColor {
    fn from(value: LabelColor) -> Self {
        RealLabelColor::from(value.to_string())
    }
}

impl From<RealLabelColor> for LabelColor {
    fn from(value: RealLabelColor) -> Self {
        LabelColor::from(value.to_string())
    }
}

impl From<String> for LabelColor {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl From<&str> for LabelColor {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct MailSettings {
    pub almost_all_mail: AlmostAllMail,
    pub attach_public_key: bool,
    pub auto_delete_spam_and_trash_days: Option<u32>,
    pub auto_save_contacts: bool,
    pub block_sender_confirmation: Option<bool>,
    pub composer_mode: ComposerMode,
    pub confirm_link: bool,
    pub delay_send_seconds: u32,
    pub display_name: String,
    pub draft_mime_type: MimeType,
    pub enable_folder_color: bool,
    pub font_face: Option<String>,
    pub hide_remote_images: bool,
    pub hide_embedded_images: bool,
    pub hide_sender_images: bool,
    pub image_proxy: u32,
    pub inherit_parent_folder_color: bool,
    pub message_buttons: MessageButtons,
    pub mobile_settings: Option<MobileSettings>,
    pub next_message_on_move: Option<NextMessageOnMove>,
    pub num_message_per_page: u32,
    pub pgp_scheme: PgpScheme,
    pub pm_signature: PmSignature,
    pub pm_signature_referral_link: bool,
    pub prompt_pin: bool,
    pub receive_mime_type: MimeType,
    pub right_to_left: ComposerDirection,
    pub shortcuts: bool,
    pub show_images: ShowImages,
    pub show_mime_type: MimeType,
    pub show_moved: ShowMoved,
    pub sign: bool,
    pub signature: String,
    pub spam_action: Option<SpamAction>,
    pub sticky_labels: bool,
    pub submission_access: bool,
    pub swipe_left: SwipeAction,
    pub swipe_right: SwipeAction,
    pub theme: String,
    pub view_layout: ViewLayout,
    pub view_mode: ViewMode,
}

impl From<RealMailSettings> for MailSettings {
    fn from(value: RealMailSettings) -> Self {
        MailSettings {
            almost_all_mail: value.almost_all_mail.into(),
            attach_public_key: value.attach_public_key,
            auto_delete_spam_and_trash_days: value.auto_delete_spam_and_trash_days,
            auto_save_contacts: value.auto_save_contacts,
            block_sender_confirmation: value.block_sender_confirmation,
            composer_mode: value.composer_mode.into(),
            confirm_link: value.confirm_link,
            delay_send_seconds: value.delay_send_seconds,
            display_name: value.display_name,
            draft_mime_type: value.draft_mime_type.into(),
            enable_folder_color: value.enable_folder_color,
            font_face: value.font_face,
            hide_remote_images: value.hide_remote_images,
            hide_embedded_images: value.hide_embedded_images,
            hide_sender_images: value.hide_sender_images,
            image_proxy: value.image_proxy.0,
            inherit_parent_folder_color: value.inherit_parent_folder_color,
            message_buttons: value.message_buttons.into(),
            mobile_settings: value.mobile_settings.map(Into::into),
            next_message_on_move: value.next_message_on_move.map(Into::into),
            num_message_per_page: value.num_message_per_page,
            pgp_scheme: value.pgp_scheme.into(),
            pm_signature: value.pm_signature.into(),
            pm_signature_referral_link: value.pm_signature_referral_link,
            prompt_pin: value.prompt_pin,
            receive_mime_type: value.receive_mime_type.into(),
            right_to_left: value.right_to_left.into(),
            shortcuts: value.shortcuts,
            show_images: value.show_images.into(),
            show_mime_type: value.show_mime_type.into(),
            show_moved: value.show_moved.into(),
            sign: value.sign,
            signature: value.signature,
            spam_action: value.spam_action.map(Into::into),
            sticky_labels: value.sticky_labels,
            submission_access: value.submission_access,
            swipe_left: value.swipe_left.into(),
            swipe_right: value.swipe_right.into(),
            theme: value.theme,
            view_layout: value.view_layout.into(),
            view_mode: value.view_mode.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct Message {
    pub id: Id,
    pub conversation_id: Id,
    pub address_id: Id,
    pub attachments_metadata: Vec<AttachmentMetadata>,
    pub bcc_list: Vec<MessageRecipient>,
    pub cc_list: Vec<MessageRecipient>,
    pub location: Option<ExclusiveLocation>,
    pub expiration_time: UnixTimestamp,
    pub flags: MessageFlags,
    pub is_forwarded: bool,
    pub is_replied: bool,
    pub is_replied_all: bool,
    pub num_attachments: u32,
    pub display_order: u64,
    pub sender: MessageSender,
    pub size: u64,
    pub snoozed_until: Option<UnixTimestamp>,
    pub display_snooze_reminder: bool,
    pub subject: String,
    pub time: UnixTimestamp,
    pub to_list: Vec<MessageRecipient>,
    pub unread: bool,
    pub custom_labels: Vec<InlineCustomLabel>,
    pub starred: bool,
    pub avatar: AvatarInformation,
    pub is_draft: bool,
    pub is_scheduled: bool,
    pub can_reply: bool,
}

impl From<RealMessage> for Message {
    fn from(value: RealMessage) -> Self {
        let starred = value.is_starred();
        let avatar = RealAvatarInformation::from(&value.sender);
        let is_draft = value.is_draft();
        let is_scheduled = value.is_scheduled_for_send();
        let can_reply = value.can_reply();
        let display_snooze_reminder = value.display_snooze_reminder();
        let snoozed_until = value.snoozed_until();

        Message {
            id: value.id().into(),
            conversation_id: value.local_conversation_id.unwrap().into(),
            address_id: value.local_address_id.into(),
            attachments_metadata: value.get_attachment_metadata().map_vec(),
            bcc_list: value.bcc_list.value.map_vec(),
            cc_list: value.cc_list.value.map_vec(),
            location: value.location.map(Into::into),
            expiration_time: value.expiration_time.into(),
            flags: value.flags.into(),
            is_forwarded: value.is_forwarded,
            is_replied: value.is_replied,
            is_replied_all: value.is_replied_all,
            num_attachments: value.num_attachments,
            display_order: value.display_order,
            sender: value.sender.into(),
            size: value.size,
            snoozed_until: snoozed_until.map(Into::into),
            display_snooze_reminder,
            subject: value.subject,
            time: if display_snooze_reminder {
                value.snooze_time.into()
            } else {
                value.time.into()
            },
            to_list: value.to_list.value.map_vec(),
            unread: value.unread,
            custom_labels: value.custom_labels.map_vec(),
            is_draft,
            is_scheduled,
            starred,
            avatar: avatar.into(),
            can_reply,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageSender {
    pub address: String,
    pub bimi_selector: Option<String>,
    pub display_sender_image: bool,
    pub is_proton: bool,
    pub is_simple_login: bool,
    pub name: String,
}

impl From<MessageSender> for RealMessageSender {
    fn from(value: MessageSender) -> Self {
        RealMessageSender {
            address: value.address.into(),
            bimi_selector: value.bimi_selector,
            display_sender_image: value.display_sender_image,
            is_proton: value.is_proton,
            is_simple_login: value.is_simple_login,
            name: value.name.into(),
        }
    }
}

impl From<RealMessageSender> for MessageSender {
    fn from(value: RealMessageSender) -> Self {
        MessageSender {
            address: value.address.into_clear_text_string(),
            bimi_selector: value.bimi_selector,
            display_sender_image: value.display_sender_image,
            is_proton: value.is_proton,
            is_simple_login: value.is_simple_login,
            name: value.name.into_clear_text_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageRecipient {
    pub address: String,
    pub is_proton: bool,
    pub name: String,
    pub group: Option<String>,
}

impl From<RealMessageRecipient> for MessageRecipient {
    fn from(value: RealMessageRecipient) -> Self {
        Self {
            address: value.address.into_clear_text_string(),
            is_proton: value.is_proton,
            name: value.name.into_clear_text_string(),
            group: value.group.into_option(),
        }
    }
}

impl From<MessageRecipient> for RealMessageRecipient {
    fn from(value: MessageRecipient) -> Self {
        if let Some(name) = &value.group {
            assert!(!name.is_empty(), "We got passed in an invalid group");
        }
        Self {
            address: value.address.into(),
            is_proton: value.is_proton,
            name: value.name.into(),
            group: MaybeEmptyString::from_option(value.group),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageReplyTo {
    pub address: String,
    pub name: String,
}

impl From<RealMessageReplyTo> for MessageReplyTo {
    fn from(value: RealMessageReplyTo) -> Self {
        Self {
            address: value.address.into_clear_text_string(),
            name: value.name.into_clear_text_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageFlags {
    value: u64,
}

impl From<MessageFlags> for RealMessageFlags {
    fn from(value: MessageFlags) -> Self {
        RealMessageFlags::from_bits_truncate(value.value)
    }
}

impl From<RealMessageFlags> for MessageFlags {
    fn from(value: RealMessageFlags) -> Self {
        MessageFlags {
            value: value.bits(),
        }
    }
}

/// Parameters to filter/search messages with a given criteria.
#[derive(Clone, Debug, SmartDefault, UniffiRecord)]
pub struct MessageSearchOptions {
    /// Filter on address ID.
    pub address_id: Option<Id>,

    /// If `true`, return only messages which have attachments. If `false`,
    /// return only messages which have no attachments.
    pub attachments: Option<bool>,

    /// If `true`, automatically convert simple queries to wildcarded versions,
    /// such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,

    /// Keyword search of `BCC` field.
    pub bcc: Option<String>,

    /// UNIX timestamp to filter messages at or later than timestamp.
    pub begin: Option<UnixTimestamp>,

    /// Return only messages newer, in creation time (NOT timestamp), than
    /// the specified message ID.
    pub begin_id: Option<Id>,

    /// Keyword search of CC field.
    pub cc: Option<String>,

    /// Filter messages by conversation ID.
    pub conversation_id: Option<Id>,

    /// If `true`, sort results descending. If `false`, sort ascending.
    pub desc: Option<bool>,

    /// UNIX timestamp to filter messages at or earlier than timestamp.
    pub end: Option<UnixTimestamp>,

    /// Return only messages older, in creation time (NOT timestamp), than the
    /// specified message ID.
    pub end_id: Option<Id>,

    /// Return only messages with the specified anchor.
    pub anchor: Option<UnixTimestamp>,

    /// Return only messages with the specified anchor ID.
    pub anchor_id: Option<Id>,

    /// Filter on external ID.
    // TODO: Document this properly.
    pub external_id: Option<String>,

    /// Keyword search `From` field.
    pub from: Option<String>,

    /// Filter on the given message IDs.
    pub ids: Option<Vec<Id>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label IDs to filter on.
    pub label_ids: Option<Vec<Id>>,

    /// The number of messages to return.
    pub limit: Option<u64>,

    /// Page index.
    pub page: u64,

    /// Number of elements per page.
    #[default(MAX_PAGE_ELEMENT_COUNT_U64)]
    pub page_size: u64,

    /// Keyword search of `To`, `CC`, and `BCC` fields.
    pub recipients: Option<Vec<String>>,

    /// Result sort mode.
    pub sort: Option<MessageMetadataSortMode>,

    /// Keyword search `Subject` field.
    pub subject: Option<String>,

    /// Keyword search of `To` field.
    pub to: Option<String>,

    /// If `true`, return only messages which are unread. If `false`, return
    /// only messages which are read.
    pub unread: Option<bool>,
}

impl MessageSearchOptions {
    pub async fn into_api_options(self, tether: &Tether) -> Result<GetMessagesOptions, StashError> {
        let ids = match self.ids {
            Some(local_ids) => {
                let mut ids = Vec::with_capacity(local_ids.len());
                for id in &local_ids {
                    if let Some(resolved_id) =
                        RealMessage::local_id_counterpart(LocalMessageId::from(*id), tether).await?
                    {
                        ids.push(resolved_id);
                    }
                }
                if ids.is_empty() { None } else { Some(ids) }
            }
            None => None,
        };
        let label_ids = match self.label_ids {
            Some(local_ids) => {
                let mut ids = Vec::with_capacity(local_ids.len());
                for id in &local_ids {
                    if let Some(resolved_id) =
                        RealLabel::local_id_counterpart(LocalLabelId::from(*id), tether).await?
                    {
                        ids.push(resolved_id);
                    }
                }
                if ids.is_empty() { None } else { Some(ids) }
            }
            None => None,
        };

        Ok(GetMessagesOptions {
            address_id: match self.address_id {
                Some(id) => {
                    RealAddress::local_id_counterpart(LocalAddressId::from(id), tether).await?
                }
                None => None,
            },
            attachments: self.attachments,
            auto_wildcard: self.auto_wildcard,
            bcc: self.bcc,
            begin: self.begin.map(|v| v.0),
            begin_id: match self.begin_id {
                Some(id) => {
                    RealMessage::local_id_counterpart(LocalMessageId::from(id), tether).await?
                }
                None => None,
            },
            cc: self.cc,
            conversation_id: match self.conversation_id {
                Some(id) => {
                    RealConversation::local_id_counterpart(LocalConversationId::from(id), tether)
                        .await?
                        .map(|v| vec![v])
                }
                None => None,
            },
            desc: self.desc,
            end: self.end.map(|v| v.0),
            end_id: match self.end_id {
                Some(id) => {
                    RealMessage::local_id_counterpart(LocalMessageId::from(id), tether).await?
                }
                None => None,
            },
            anchor: self.anchor.map(|v| v.0),
            anchor_id: match self.anchor_id {
                Some(id) => {
                    RealMessage::local_id_counterpart(LocalMessageId::from(id), tether).await?
                }
                None => None,
            },
            external_id: self.external_id,
            from: self.from,
            ids,
            keyword: self.keyword,
            label_id: label_ids,
            limit: self.limit,
            page: self.page,
            page_size: self.page_size,
            recipients: self.recipients,
            sort: self.sort.map(Into::into),
            subject: self.subject,
            to: self.to,
            unread: self.unread,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, UniffiEnum)]
pub enum MobileAction {
    Archive,
    Forward,
    Label,
    Move,
    Print,
    Reply,
    ReportPhishing,
    Snooze,
    Spam,
    ToggleLight,
    ToggleRead,
    ToggleStar,
    Trash,
    ViewHeaders,
    ViewHTML,
}

impl MobileAction {
    #[must_use]
    pub fn from_real(value: &RealMobileAction) -> Option<Self> {
        match value {
            RealMobileAction::Archive => Some(Self::Archive),
            RealMobileAction::Forward => Some(Self::Forward),
            RealMobileAction::Label => Some(Self::Label),
            RealMobileAction::Move => Some(Self::Move),
            RealMobileAction::Print => Some(Self::Print),
            RealMobileAction::Reply => Some(Self::Reply),
            RealMobileAction::ReportPhishing => Some(Self::ReportPhishing),
            RealMobileAction::Snooze => Some(Self::Snooze),
            RealMobileAction::Spam => Some(Self::Spam),
            RealMobileAction::ToggleLight => Some(Self::ToggleLight),
            RealMobileAction::ToggleRead => Some(Self::ToggleRead),
            RealMobileAction::ToggleStar => Some(Self::ToggleStar),
            RealMobileAction::Trash => Some(Self::Trash),
            RealMobileAction::ViewHeaders => Some(Self::ViewHeaders),
            RealMobileAction::ViewHTML => Some(Self::ViewHTML),
            RealMobileAction::Remind
            | RealMobileAction::SaveAttachments
            | RealMobileAction::SenderEmails
            | RealMobileAction::SavePDF
            | RealMobileAction::Other(_) => None,
        }
    }
}

impl From<MobileAction> for RealMobileAction {
    fn from(value: MobileAction) -> Self {
        match value {
            MobileAction::Archive => Self::Archive,
            MobileAction::Forward => Self::Forward,
            MobileAction::Label => Self::Label,
            MobileAction::Move => Self::Move,
            MobileAction::Print => Self::Print,
            MobileAction::Reply => Self::Reply,
            MobileAction::ReportPhishing => Self::ReportPhishing,
            MobileAction::Snooze => Self::Snooze,
            MobileAction::Spam => Self::Spam,
            MobileAction::ToggleLight => Self::ToggleLight,
            MobileAction::ToggleRead => Self::ToggleRead,
            MobileAction::ToggleStar => Self::ToggleStar,
            MobileAction::Trash => Self::Trash,
            MobileAction::ViewHeaders => Self::ViewHeaders,
            MobileAction::ViewHTML => Self::ViewHTML,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MobileSetting {
    pub actions: Vec<MobileAction>,
    pub is_custom: bool,
}

impl From<MobileSetting> for RealMobileSetting {
    fn from(value: MobileSetting) -> Self {
        RealMobileSetting {
            actions: value.actions.map_vec(),
            is_custom: value.is_custom,
        }
    }
}

impl From<RealMobileSetting> for MobileSetting {
    fn from(value: RealMobileSetting) -> Self {
        MobileSetting {
            actions: value
                .actions
                .iter()
                .filter_map(MobileAction::from_real)
                .collect_vec(),
            is_custom: value.is_custom,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MobileSettings {
    pub conversation_toolbar: MobileSetting,
    pub list_toolbar: MobileSetting,
    pub message_toolbar: MobileSetting,
}

impl From<MobileSettings> for RealMobileSettings {
    fn from(value: MobileSettings) -> Self {
        RealMobileSettings {
            conversation_toolbar: value.conversation_toolbar.into(),
            list_toolbar: value.list_toolbar.into(),
            message_toolbar: value.message_toolbar.into(),
        }
    }
}

impl From<RealMobileSettings> for MobileSettings {
    fn from(value: RealMobileSettings) -> Self {
        MobileSettings {
            conversation_toolbar: value.conversation_toolbar.into(),
            list_toolbar: value.list_toolbar.into(),
            message_toolbar: value.message_toolbar.into(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct LabelAsOutput {
    pub input_label_is_empty: bool,
    pub undo: Option<Arc<Undo>>,
}

impl From<RealLabelAsOutput> for LabelAsOutput {
    fn from(value: RealLabelAsOutput) -> Self {
        Self {
            input_label_is_empty: value.input_label_is_empty,
            undo: value.undo.map(|undo| Arc::new(undo.into())),
        }
    }
}

#[derive(uniffi::Object)]
pub struct Undo(Mutex<Option<RealUndo>>);

#[uniffi_export]
impl Undo {
    async fn undo(&self, ctx: Arc<MailUserSession>) -> Result<(), ActionError> {
        let Some(output) = self.0.lock().take() else {
            warn!("already undone");
            return Ok(());
        };

        let ctx = ctx.ctx()?;

        uniffi_async(async move {
            let mut tether = ctx.user_stash().connection().await?;
            output.undo(ctx.action_queue(), &mut tether).await?;
            Ok::<_, ProtonMailError>(())
        })
        .await
        .map_err(ActionError::from)
    }
}

impl From<RealUndo> for Undo {
    fn from(value: RealUndo) -> Self {
        Self(Mutex::new(Some(value)))
    }
}
