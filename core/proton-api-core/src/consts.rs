#![allow(clippy::unreadable_literal)]
///  Missing field is required
pub const INVALID_REQUIREMENTS: u32 = 2000;
///  The field is not correct (most generic error)
pub const INVALID_VALUE: u32 = 2001;
///  Type is not correct (string/int/etc.)
pub const INVALID_TYPE: u32 = 2002;
///  The field is too small/big
pub const VALUE_OUT_OF_BOUNDS: u32 = 2003;
///  The field must be null but it is not
pub const NOT_NULL: u32 = 2004;
///  The field must be empty but it is not
pub const NOT_EMPTY: u32 = 2005;
///  The field must be true but is not
pub const NOT_TRUE: u32 = 2006;
///  The field must be false but is not
pub const NOT_FALSE: u32 = 2007;
///  The field must be equal to another value but is not
pub const NOT_EQUAL: u32 = 2008;
///  The field must not be equal to another value but it is
pub const EQUAL: u32 = 2009;
///  The field must be equal to the value of another field
pub const NOT_SAME_AS_FIELD: u32 = 2010;
///  The field or action not allowed
pub const NOT_ALLOWED: u32 = 2011;
///  Invalid number of items (too few/many)
pub const INVALID_NUMBER: u32 = 2021;
///  Invalid length (too short/long)
pub const INVALID_LENGTH: u32 = 2022;
///  The field is too short
pub const TOO_SHORT: u32 = 2023;
///  The field is too long
pub const TOO_LONG: u32 = 2024;
pub const HASH_NOT_EQUAL: u32 = 2025;
///  The current user does not have necessary permissions.
pub const PERMISSION_DENIED: u32 = 2026;
///  The action cannot be executed with the user's subscription scope.
pub const INSUFFICIENT_SCOPE: u32 = 2027;
///  The action is explicitly forbidden for this user.
pub const BANNED: u32 = 2028;
///  The user has no password reset methods enabled
pub const NO_RESET_METHODS: u32 = 2029;
///  Any upload failure
pub const UPLOAD_FAILURE: u32 = 2030;
///  Payload is too large
pub const PAYLOAD_TOO_LARGE: u32 = 2031;
///  Blocked due to feature being disabled, clients are encouraged to refetch feature flags
pub const FEATURE_DISABLED: u32 = 2032;
pub const EMAIL_FORMAT: u32 = 2050;
pub const IP_FORMAT: u32 = 2051;
pub const URL_FORMAT: u32 = 2052;
pub const CURRENCY_FORMAT: u32 = 2053;
pub const LOCALE_FORMAT: u32 = 2054;
pub const DATE_FORMAT: u32 = 2055;
pub const JSON_FORMAT: u32 = 2056;
pub const MIME_FORMAT: u32 = 2057;
pub const PHONE_FORMAT: u32 = 2058;
pub const DOMAIN_FORMAT: u32 = 2059;
pub const PGP_FORMAT: u32 = 2060;
pub const ID_FORMAT: u32 = 2061;
pub const HEX_FORMAT: u32 = 2062;
pub const BASE64_FORMAT: u32 = 2063;
pub const VERSION_FORMAT: u32 = 2064;
pub const IMAGE_FORMAT: u32 = 2065;
///  The field did not pass the regex validation
pub const REGEX_ERROR: u32 = 2120;
///  The field must not exist within the given database table
pub const ALREADY_EXISTS: u32 = 2500;
///  The field must exist on a given database table but does not, or an entity with a given key does not exist
pub const NOT_EXISTS: u32 = 2501;
///  The operation cannot be performed as some lock prevents it
pub const IS_LOCKED: u32 = 2502;
///  A timeout was reached
pub const TIMEOUT: u32 = 2503;
///  An error that is not `ALREADY_EXISTS` in an INSERT query was encountered
pub const INSERT_FAILED: u32 = 2504;
///  An error that is not `NOT_EXISTS` happened in a SELECT query
pub const SELECT_FAILED: u32 = 2505;
///  The model exists but its current state doesn't allow to execute the action
pub const INCOMPATIBLE_STATE: u32 = 2511;
///  The lock id provided is not from that document
pub const BAD_LOCK_ID: u32 = 2512;
///  An external service or dependency is unavailable or turned off
pub const PROVIDER_UNAVAILABLE: u32 = 2900;
///  An external service or dependency has thrown a configuration error
pub const PROVIDER_MISCONFIGURED: u32 = 2901;
///  A request to external service or dependency has failed
pub const PROVIDER_FAILED: u32 = 2902;
///  A request to external service was blocked before being sent
pub const PROVIDER_BLOCKED: u32 = 2903;
pub const ADMIN_DISABLE_2FA_USER_INVALID: u32 = 25776;

pub mod core_bundle {
    ///  Invalid semver version
    pub const APP_VERSION_INVALID: u32 = 5002;
    ///  Version of the "appversion" is out of date, trigger force upgrade
    pub const APP_VERSION_BAD: u32 = 5003;
    ///  Ask the user to upgrade to a newer version of the app to use external account
    pub const APP_VERSION_TOO_OLD_FOR_EXTERNAL_ACCOUNTS: u32 = 5098;
    ///  Ask the user to create a proton address in the webclient before using this app
    pub const APP_VERSION_NOT_SUPPORTED_FOR_EXTERNAL_ACCOUNTS: u32 = 5099;
    ///  The API is in offline mode
    pub const API_OFFLINE: u32 = 7001;
    ///  Re-type password - Old password incorrect, disable login button
    pub const PASSWORD_WRONG: u32 = 8002;
    ///  Display too many children session for OAUTH clients localized error with knowledge base
    pub const TOO_MANY_CHILDREN: u32 = 8003;
    ///  The client is not allowed to create a session
    pub const SESSION_NOT_ALLOWED: u32 = 8004;
    ///  Display JWT expired customized error
    pub const JWT_EXPIRED: u32 = 8005;
    ///  Redirect user to the login page, informing that the link is used
    pub const JWT_REDIRECT_LOGIN: u32 = 8006;
    pub const AUTH_SWITCH_TO_SSO: u32 = 8100;
    pub const AUTH_SWITCH_TO_SRP: u32 = 8101;
    ///  Show the human verification modal as documented here: <https:confluence.protontech.ch/display/CP/Human+Verification>
    pub const HUMAN_VERIFICATION_REQUIRED: u32 = 9001;
    ///  Prompt device verification as documented here: <https:confluence.protontech.ch/display/CP/Tech+Proposal%3A+Proof+of+work>
    pub const DEVICE_VERIFICATION_REQUIRED: u32 = 9002;
    pub const SCOPE_MISSING_UNEXPECTED: u32 = 9100;
    pub const SCOPE_REAUTH_LOCKED: u32 = 9101;
    pub const SCOPE_REAUTH_PASSWORD: u32 = 9102;
    pub const SCOPE_DELINQUENT: u32 = 9103;
    pub const SCOPE_VERIFICATION: u32 = 9104;
    pub const SCOPE_LOGGEDIN_CREDENTIALLESS: u32 = 9105;
    pub const SCOPE_UNAUTH_SESSION: u32 = 9106;
    ///  Display an upsell modal after a free trial has expired
    pub const TRIAL_EXPIRED: u32 = 9200;
    ///  Detect login failed
    pub const AUTH_AUTH_ACCOUNT_FAILED_GENERIC: u32 = 10001;
    ///  Detect this account is disabled
    pub const AUTH_AUTH_ACCOUNT_DELETED: u32 = 10002;
    ///  Show the abuse fraud modal
    pub const AUTH_AUTH_ACCOUNT_DISABLED: u32 = 10003;
    ///  Specific flow for upgrade
    pub const AUTH_AUTH_PAID_PLAN_REQUIRED: u32 = 10004;
    ///  Hide the refresh token error
    pub const AUTH_REFRESH_TOKEN_INVALID: u32 = 10013;
    ///  Hide the refresh token error
    pub const AUTH_COOKIES_REFRESH_INVALID: u32 = 10021;
    ///  Detect superfluous 2FA submission
    pub const AUTH_2FA_NOT_ENABLED: u32 = 10100;
    ///  Detect invalid 2FA
    pub const AUTH_2FA_INPUT_INVALID: u32 = 10101;
    ///  Detect invalid 2FA
    pub const AUTH_2FA_TOKEN_INVALID: u32 = 10102;
    ///  Could not create credential-less user
    pub const ACCOUNT_CREDENTIALLESS_INVALID: u32 = 10200;
    ///  Specific device not found
    pub const AUTH_DEVICE_NOT_FOUND: u32 = 10300;
    ///  Activate device using other devices or backup password
    pub const AUTH_DEVICE_NOT_ACTIVE: u32 = 10301;
    ///  Delete local secret and create new device
    pub const AUTH_DEVICE_TOKEN_INVALID: u32 = 10302;
    ///  Delete local secret and create new device
    pub const AUTH_DEVICE_REJECTED: u32 = 10303;
    ///  Set up user by creating keys or change the password (done by admin)
    pub const MISSING_SRP_PARAMETERS: u32 = 10400;
    pub const USER_UPDATE_EMAIL_EMAIL_INVALID: u32 = 12006;
    pub const USER_UPDATE_2FA_CONFIRMATION_FAILED: u32 = 12060;
    pub const USER_UPDATE_2FA_INPUT_INVALID: u32 = 12061;
    ///  Detect failed human verification
    pub const USER_CREATE_TOKEN_INVALID: u32 = 12087;
    ///  Used for `Force2FA` and `ForcePasswordChange`
    pub const USER_RESTRICTED_STATE: u32 = 12100;
    ///  Display custom please use a non-ProtonMail email address on signup
    pub const USER_CODE_EMAIL_INVALID_PROTONMAIL: u32 = 12220;
    ///  Display custom invalid email address
    pub const USER_CODE_EMAIL_INVALID: u32 = 12221;
    pub const USER_QUOTA_EXCEEDED: u32 = 12403;
    ///  Handle the "Invalid email" error
    pub const KEY_GET_INPUT_INVALID: u32 = 33101;
    ///  Handle the "Invalid email" error
    pub const KEY_GET_ADDRESS_MISSING: u32 = 33102;
    ///  Handle the "Invalid email" error
    pub const KEY_GET_DOMAIN_EXTERNAL: u32 = 33103;
    ///  Handle the "Invalid email" error
    pub const KEY_GET_INVALID_KT: u32 = 33104;
    ///  Display custom error on mnemonic ban on `WebClient`
    pub const BANS_MNEMONIC: u32 = 85071;
    pub const BANS_VERIFY_EMAIL: u32 = 85102;
}

pub mod mail {
    ///  Create a new message when the user deletes a draft in another tab
    pub const MESSAGE_UPDATE_DRAFT_NOT_EXIST: u32 = 15033;
    ///  Close the composer where the message has already been sent, race condition?
    pub const MESSAGE_UPDATE_DRAFT_NOT_DRAFT: u32 = 15034;
    ///  Hide this error message
    pub const MESSAGE_VALIDATE_KEY_ID_NOT_ASSOCIATED: u32 = 15213;
    ///  Show a custom search syntax error
    pub const MESSAGE_SEARCH_QUERY_SYNTAX: u32 = 15225;
    /// Mail Refresh the list
    pub const INCOMING_DEFAULT_UPDATE_NOT_EXIST: u32 = 35023;
    ///  Hanldle the message too large to import
    pub const IMPORT_MESSAGE_TOO_LARGE: u32 = 36022;
    ///  Hide the create filter modal
    pub const FILTER_CREATE_TOO_MANY_ACTIVE: u32 = 50016;
}

pub mod payments {
    ///  Respond to Apple payments issue
    pub const PAYMENTS_SUBSCRIPTION_AMOUNT_MISMATCH: u32 = 22101;
    ///  Check if user has any existing subscriptions before allowing in app purchases
    pub const PAYMENTS_SUBSCRIPTION_NOT_EXISTS: u32 = 22110;
    ///  Create a waitlist
    pub const PAYMENTS_SUBSCRIPTION_TOO_LITTLE_SPACE: u32 = 22117;
    pub const PAYMENTS_STRIPE_3DS_REQUIRED: u32 = 22718;
    ///  Show a specific paypal network error
    pub const PAYMENTS_PAYPAL_CONNECTION_EXCEPTION: u32 = 22802;
    ///  Respond to Apple payments issue
    pub const PAYMENTS_APPLE_CURRENCY_INVALID: u32 = 22915;
}

pub mod vpn {
    /// Handle invalid profile ID (when updating)
    pub const VPN_PROFILE_UPDATE_ID_INVALID: u32 = 86062;
    /// Handle invalid profile ID (when deleting)
    pub const VPN_PROFILE_DELETE_ID_INVALID: u32 = 86063;
    /// Handle a profile with this name already exists (when creating or updating)
    pub const VPN_PROFILE_NAME_ALREADY_USED: u32 = 86065;
    /// When the current action cannot be performed while connected to one of our servers
    pub const VPN_CONNECTION_DISALLOWED: u32 = 86081;
    /// When key is not known from the DB
    pub const VPN_KEY_NOT_FOUND: u32 = 86100;
    /// When trying to connect using an expired certificate
    pub const VPN_EXPIRED_CERTIFICATE: u32 = 86101;
    /// When trying to connect using a revoked certificate
    pub const VPN_REVOKED_CERTIFICATE: u32 = 86102;
    /// When trying to connect on more than 1 server using 1 certificate
    pub const VPN_DUPLICATED_CERTIFICATE: u32 = 86103;
    /// When trying to connect restricted server or with a broken certificate
    pub const VPN_FAILED_CERTIFICATE_VERIFICATION: u32 = 86104;
    /// When trying to connect with a certificate incorrectly signed, signed by an unknown, expired or revoked authority
    pub const VPN_FAILED_CERTIFICATE_SIGNATURE_VALIDATION: u32 = 86105;
    /// When trying to connect more devices than allowed simultaneously
    pub const VPN_SESSIONS_COUNT_OVER_QUOTA: u32 = 86110;
    /// When user has free plan and try to connect more devices than allowed simultaneously
    pub const VPN_SESSIONS_COUNT_OVER_QUOTA_FREE: u32 = 86111;
    /// When user has basic plan and try to connect more devices than allowed simultaneously
    pub const VPN_SESSIONS_COUNT_OVER_QUOTA_BASIC: u32 = 86112;
    /// When user has plus plan and try to connect more devices than allowed simultaneously
    pub const VPN_SESSIONS_COUNT_OVER_QUOTA_PLUS: u32 = 86113;
    /// When user has visionary plan and try to connect more devices than allowed simultaneously
    pub const VPN_SESSIONS_COUNT_OVER_QUOTA_VISIONARY: u32 = 86114;
    /// When user has pro plan and try to connect more devices than allowed simultaneously
    pub const VPN_SESSIONS_COUNT_OVER_QUOTA_PRO: u32 = 86115;
    /// Unknown error on the exit VPN server system
    pub const VPN_SERVER_SYSTEM_ERROR: u32 = 86150;
    /// When connected to a server not included in the user plan
    pub const VPN_SERVER_NOT_ALLOWED_WITH_CURRENT_PLAN: u32 = 86151;
    /// When the plan that would allow to connect to the requested server has pending invoices
    pub const VPN_SERVER_NOT_ALLOWED_WHILE_INVOICE_IS_PENDING: u32 = 86152;
    /// When the user is torrenting while the plan does not allow it
    pub const VPN_TORRENT_NOT_ALLOWED: u32 = 86153;
    /// When the user has an abuser status
    pub const VPN_BAD_USER_BEHAVIOR: u32 = 86154;
    /// When the user first need to assign VPN connections to use the client
    pub const VPN_NO_CONNECTION_ASSIGNED: u32 = 86300;
    /// When the user needs to buy more dedicated IPs to continue
    pub const VPN_INSUFFICIENT_DEDICATED_IPS: u32 = 86301;
}

pub mod drive {
    pub const UNKNOWN_ERROR: u32 = 200000;
    pub const INSUFFICIENT_QUOTA: u32 = 200001;
    pub const INSUFFICIENT_SPACE: u32 = 200002;
    pub const MAX_FILE_SIZE_FOR_FREE_USER: u32 = 200003;
    pub const INSUFFICIENT_VOLUME_QUOTA: u32 = 200100;
    pub const INSUFFICIENT_DEVICE_QUOTA: u32 = 200101;
    pub const ADDRESS_NOT_FOUND: u32 = 200200;
    pub const ALREADY_MEMBER_OF_SHARE_IN_VOLUME_WITH_ANOTHER_ADDRESS: u32 = 200201;
    pub const SHARE_ADDRESS_TYPE_GROUP_NOT_YET_SUPPORTED: u32 = 200202;
    pub const TOO_MANY_CHILDREN: u32 = 200300;
    pub const NESTING_TOO_DEEP: u32 = 200301;
    pub const SHARE_URL_COPYRIGHT_INFRINGEMENT: u32 = 200400;
    pub const ENCRYPTION_VERIFICATION_FAILED: u32 = 200501;
    pub const SIGNATURE_VERIFICATION_FAILED: u32 = 200502;
    pub const INSUFFICIENT_INVITATION_QUOTA: u32 = 200600;
    pub const INSUFFICIENT_SHARE_QUOTA: u32 = 200601;
    pub const INSUFFICIENT_SHARE_JOINED_QUOTA: u32 = 200602;
    pub const REVISION_NOT_ENABLED_FOR_DOCUMENTS: u32 = 200700;
    pub const FILE_CREATION_NOT_ENABLED_FOR_DOCUMENTS: u32 = 200701;
    pub const INSUFFICIENT_BOOKMARKS_QUOTA: u32 = 200800;
    pub const USER_SETTINGS_NOT_UPDATED: u32 = 200900;
    pub const USER_SETTINGS_NOT_ENABLED: u32 = 200901;
}

pub mod pass {
    pub const NOT_LATEST_KEY: u32 = 300001;
    pub const NOT_LATEST_REVISION: u32 = 300002;
    pub const INVALID_SIGNATURE: u32 = 300003;
    pub const DELETED_SHARE: u32 = 300004;
    pub const ROTATION_PAYLOAD_INCOMPLETE: u32 = 300005;
    pub const MISSING_KEYS: u32 = 300006;
    pub const RESOURCE_LIMIT_EXCEEDED: u32 = 300007;
    pub const SESSION_LOCKED: u32 = 300008;
}

pub mod kt_bundle {
    pub const SKL_FORMAT: u32 = 400001;
    pub const VERIFIED_EPOCH_NOT_FOUND: u32 = 400100;
    pub const ADDRESS_LIST_NOT_FOUND: u32 = 400101;
}
