use derive_more::From;
use muon::{ProtonRequest, ProtonResponse, common::Sender};
use proton_core_api::{
    service::ApiServiceResult,
    services::proton::{PostAuthInfoRequest, PostAuthInfoResponse, ProtonAuth as _},
};
use proton_core_common::device::DeviceInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Info needed to construct the challenge payload.
#[derive(Debug, Clone, Default)]
pub struct ChallengeInfo {
    /// Product name to be used in a challenge payload (e.g. `mail`).
    pub product_name: String,
    /// Device fingerprint.
    pub device_info: Option<DeviceInfo>,
    /// User behaviour while entering the recovery method (if applicable).
    pub recovery_behavior: Option<Behavior>,
    /// User behaviour while entering the username (if applicable).
    pub username_behavior: Option<Behavior>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, From)]
pub struct ChallengePayload {
    #[serde(flatten)]
    frames: HashMap<String, PayloadFrame>,
}

/// Challenge payload frame containing device fingerprint and user behaviour.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, From)]
#[serde(tag = "v")]
#[serde(rename_all = "PascalCase")]
pub enum PayloadFrame {
    #[serde(rename = "2.2.0")]
    V2_2(PayloadFrameV2_2),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PayloadFrameV2_2 {
    /// Frame's metadata.
    #[serde(rename = "frame", skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PayloadFrameMetadata>,
    /// Device fingerprint.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub device_info: Option<DeviceInfo>,
    /// User behaviour on a sign up screen.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub user_behavior: Option<PayloadFrameBehavior>,
}

/// Challenge payload frame's metadata.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "name", rename_all = "camelCase")]
pub enum PayloadFrameMetadata {
    /// Frame built while user was entering the recovery method.
    Recovery,
    /// Frame built while user was entering the username.
    Username,
}

/// User activity during text input.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PayloadFrameBehavior {
    /// User activity while entering the recovery method.
    #[serde(with = "suffix_recovery")]
    Recovery(Behavior),
    /// User activity while entering the username.
    #[serde(with = "suffix_username")]
    Username(Behavior),
}

serde_with::with_suffix!(suffix_recovery "Recovery");
serde_with::with_suffix!(suffix_username "Username");

/// User activity while entering the recovery method.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Behavior {
    /// Durations (in seconds) of each focus session on the text field.
    pub time: Vec<u32>,
    /// Number of clicks / taps during user input.
    pub click: u32,
    /// Text chunks copied during user input.
    pub copy: Vec<String>,
    /// Text chunks pasted during user input.
    pub paste: Vec<String>,
    /// Characters entered during user input.
    pub keydown: Vec<String>,
}

impl ChallengePayload {
    #[must_use]
    pub fn new(challenge_info: &ChallengeInfo) -> Option<Self> {
        if challenge_info.recovery_behavior.is_none() && challenge_info.username_behavior.is_none()
        {
            if challenge_info.device_info.is_some() {
                let mut frames = HashMap::with_capacity(1);
                insert_payload_frame(&mut frames, challenge_info, None, None);
                return Some(ChallengePayload { frames });
            }

            return None;
        }

        let mut frames = HashMap::with_capacity(2);

        if let Some(behavior) = challenge_info.recovery_behavior.clone() {
            insert_payload_frame(
                &mut frames,
                challenge_info,
                Some(PayloadFrameMetadata::Recovery),
                Some(PayloadFrameBehavior::Recovery(behavior)),
            );
        }

        if let Some(behavior) = challenge_info.username_behavior.clone() {
            insert_payload_frame(
                &mut frames,
                challenge_info,
                Some(PayloadFrameMetadata::Username),
                Some(PayloadFrameBehavior::Username(behavior)),
            );
        }

        Some(ChallengePayload { frames })
    }
}

fn insert_payload_frame(
    payload: &mut HashMap<String, PayloadFrame>,
    challenge_info: &ChallengeInfo,
    metadata: Option<PayloadFrameMetadata>,
    behavior: Option<PayloadFrameBehavior>,
) {
    let id = payload.len();
    let name = format!("{}-v4-challenge-{id}", challenge_info.product_name);
    payload.insert(
        name,
        PayloadFrameV2_2 {
            metadata,
            device_info: challenge_info.device_info.clone(),
            user_behavior: behavior,
        }
        .into(),
    );
}

pub async fn get_auth_info(
    client: &impl Sender<ProtonRequest, ProtonResponse>,
    username: &str,
) -> ApiServiceResult<PostAuthInfoResponse> {
    let request = PostAuthInfoRequest {
        username: username.to_owned(),
    };

    client.post_auth_info(request).await
}

#[cfg(test)]
mod test {
    use crate::shared::challenge::{
        Behavior, ChallengeInfo, ChallengePayload, PayloadFrameBehavior, PayloadFrameMetadata,
        PayloadFrameV2_2,
    };
    use proton_core_common::device::DeviceInfo;
    use std::collections::HashMap;

    #[test]
    fn test_create_payload() {
        let device_info = DeviceInfo {
            language: "lang".into(),
            timezone: "tz".into(),
            timezone_offset: -60,
            model: "model".into(),
            brand: "brand".into(),
            codename: "code".into(),
            uuid: "uuid".into(),
            country: "country".into(),
            rooted: false,
            font_scale: "2.0".into(),
            storage: 42.0,
            dark_mode: true,
            keyboards: vec!["kb_1".into()],
        };
        let username_behavior = Behavior {
            time: vec![123],
            click: 12,
            copy: vec!["usr_cf".into()],
            paste: vec!["usr_pf".into()],
            keydown: vec!["usr_kdf".into()],
        };
        let recovery_behavior = Behavior {
            time: vec![456],
            click: 34,
            copy: vec!["rec_cf".into()],
            paste: vec!["rec_pf".into()],
            keydown: vec!["rec_kdf".into()],
        };
        let challenge_info = ChallengeInfo {
            product_name: "mail-ios".into(),
            device_info: Some(device_info.clone()),
            recovery_behavior: Some(recovery_behavior.clone()),
            username_behavior: Some(username_behavior.clone()),
        };
        let payload = ChallengePayload::new(&challenge_info);
        assert_eq!(
            payload,
            Some(ChallengePayload {
                frames: HashMap::from_iter([
                    (
                        "mail-ios-v4-challenge-0".to_string(),
                        PayloadFrameV2_2 {
                            metadata: Some(PayloadFrameMetadata::Recovery),
                            device_info: Some(device_info.clone()),
                            user_behavior: Some(PayloadFrameBehavior::Recovery(recovery_behavior)),
                        }
                        .into(),
                    ),
                    (
                        "mail-ios-v4-challenge-1".to_string(),
                        PayloadFrameV2_2 {
                            metadata: Some(PayloadFrameMetadata::Username),
                            device_info: Some(device_info.clone()),
                            user_behavior: Some(PayloadFrameBehavior::Username(username_behavior)),
                        }
                        .into(),
                    )
                ])
            })
        );
    }
}
