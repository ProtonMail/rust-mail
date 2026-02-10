use proton_account_api::shared::challenge::Behavior;

/// User activity during text input.
#[derive(uniffi::Record, Clone)]
pub struct UserBehavior {
    /// Time from form load to user providing input (in seconds).
    pub time_on_field: Vec<u32>,
    /// Number of clicks / taps during user input.
    pub click_on_field: u32,
    /// Text chunks copied during user input.
    pub copy_field: Vec<String>,
    /// Text chunks pasted during user input.
    pub paste_field: Vec<String>,
    /// Characters entered during user input.
    pub key_down_field: Vec<String>,
}

impl From<UserBehavior> for Behavior {
    fn from(value: UserBehavior) -> Self {
        Self {
            time: value.time_on_field,
            click: value.click_on_field,
            copy: value.copy_field,
            paste: value.paste_field,
            keydown: value.key_down_field,
        }
    }
}
