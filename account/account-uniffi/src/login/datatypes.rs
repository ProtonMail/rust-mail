use muon::client::flow::LoginFlowData;
use proton_core_api::{
    auth::{KeySecret, UserKeySecret},
    store::UserData,
};
use secrecy::SecretString;

/// All necessary **unencrypted** data for the migration from legacy version
/// of the app
#[derive(uniffi::Record)]
pub struct MigrationData {
    /// The name of the user.
    pub username: String,

    /// The user's display name.
    pub display_name: String,

    /// The user's primary email address.
    pub primary_addr: String,

    /// The user's **unecrypted** key secret.
    /// In Base64 format.
    ///
    pub key_secret: String,

    /// The user's ID.
    pub user_id: String,

    /// The user's unique session ID.
    pub session_id: String,

    /// The passwords mode.
    pub password_mode: PasswordMode,

    /// The refresh token. This token must be refreshed before use;
    /// once refreshed, it becomes an access token.
    pub refresh_token: String,
}

impl MigrationData {
    /// Splits the migration data into internal parts
    ///
    #[must_use]
    pub fn into_parts(self) -> (UserData, LoginFlowData, SecretString) {
        let Self {
            username,
            display_name,
            primary_addr,
            key_secret,
            user_id,
            session_id,
            password_mode,
            refresh_token,
        } = self;

        let key_secret = key_secret.as_bytes();
        (
            UserData {
                username,
                display_name,
                primary_addr,
                key_secret: UserKeySecret(KeySecret::new(key_secret.into())),
            },
            LoginFlowData {
                user_id,
                session_id,
                password_mode: match password_mode {
                    PasswordMode::One => muon::client::PasswordMode::One,
                    PasswordMode::Two => muon::client::PasswordMode::Two,
                },
            },
            SecretString::new(refresh_token),
        )
    }
}

/// Represents the password mode of an account.
///
/// Note: this is not strictly related to the auth system;
/// it is used to determine whether an account's keys are locked
/// with the primary account password or with a separate password.
#[derive(uniffi::Enum)]
pub enum PasswordMode {
    /// The account has one password.
    One,

    /// The account has two passwords.
    Two,
}
