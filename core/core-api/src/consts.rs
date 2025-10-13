#![allow(clippy::unreadable_literal)]

pub enum General {
    NoError = 1000,
    ///  Missing field is required
    InvalidRequirements = 2000,
    ///  The field is not correct (most generic error)
    InvalidValue = 2001,
    ///  Type is not correct (string/int/etc.)
    InvalidType = 2002,
    ///  The field is too small/big
    ValueOutOfBounds = 2003,
    ///  The field must be null but it is not
    NotNull = 2004,
    ///  The field must be empty but it is not
    NotEmpty = 2005,
    ///  The field must be true but is not
    NotTrue = 2006,
    ///  The field must be false but is not
    NotFalse = 2007,
    ///  The field must be equal to another value but is not
    NotEqual = 2008,
    ///  The field must not be equal to another value but it is
    Equal = 2009,
    ///  The field must be equal to the value of another field
    NotSameAsField = 2010,
    ///  The field or action not allowed
    NotAllowed = 2011,
    ///  Invalid number of items (too few/many)
    InvalidNumber = 2021,
    ///  Invalid length (too short/long)
    InvalidLength = 2022,
    ///  The field is too short
    TooShort = 2023,
    ///  The field is too long
    TooLong = 2024,
    HashNotEqual = 2025,
    ///  The current user does not have necessary permissions.
    PermissionDenied = 2026,
    ///  The action cannot be executed with the user's subscription scope.
    InsufficientScope = 2027,
    ///  The action is explicitly forbidden for this user.
    Banned = 2028,
    ///  The user has no password reset methods enabled
    NoResetMethods = 2029,
    ///  Any upload failure
    UploadFailure = 2030,
    ///  Payload is too large
    PayloadTooLarge = 2031,
    ///  Blocked due to feature being disabled, clients are encouraged to refetch feature flags
    FeatureDisabled = 2032,
    EmailFormat = 2050,
    IpFormat = 2051,
    UrlFormat = 2052,
    CurrencyFormat = 2053,
    LocaleFormat = 2054,
    DateFormat = 2055,
    JsonFormat = 2056,
    MimeFormat = 2057,
    PhoneFormat = 2058,
    DomainFormat = 2059,
    PgpFormat = 2060,
    IdFormat = 2061,
    HexFormat = 2062,
    Base64Format = 2063,
    VersionFormat = 2064,
    ImageFormat = 2065,
    ///  The field did not pass the regex validation
    RegexError = 2120,
    ///  The field must not exist within the given database table
    AlreadyExists = 2500,
    ///  The field must exist on a given database table but does not, or an entity with a given key does not exist
    NotExists = 2501,
    ///  The operation cannot be performed as some lock prevents it
    IsLocked = 2502,
    ///  A timeout was reached
    TIMEOUT = 2503,
    ///  An error that is not `ALREADY_EXISTS` in an INSERT query was encountered
    InsertFailed = 2504,
    ///  An error that is not `NOT_EXISTS` happened in a SELECT query
    SelectFailed = 2505,
    ///  The model exists but its current state doesn't allow to execute the action
    IncompatibleState = 2511,
    ///  The lock id provided is not from that document
    BadLockId = 2512,
    ///  An external service or dependency is unavailable or turned off
    ProviderUnavailable = 2900,
    ///  An external service or dependency has thrown a configuration error
    ProviderMisconfigured = 2901,
    ///  A request to external service or dependency has failed
    ProviderFailed = 2902,
    ///  A request to external service was blocked before being sent
    ProviderBlocked = 2903,
    AdminDisable2faUserInvalid = 25776,
}

pub enum CoreBundle {
    ///  Invalid semver version
    AppVersionInvalid = 5002,
    ///  Version of the "appversion" is out of date, trigger force upgrade
    AppVersionBad = 5003,
    ///  Ask the user to upgrade to a newer version of the app to use external account
    AppVersionTooOldForExternalAccounts = 5098,
    ///  Ask the user to create a proton address in the webclient before using this app
    AppVersionNotSupportedForExternalAccounts = 5099,
    ///  The API is in offline mode
    ApiOffline = 7001,
    ///  Re-type password - Old password incorrect, disable login button
    PasswordWrong = 8002,
    ///  Display too many children session for OAUTH clients localized error with knowledge base
    TooManyChildren = 8003,
    ///  The client is not allowed to create a session
    SessionNotAllowed = 8004,
    ///  Display JWT expired customized error
    JwtExpired = 8005,
    ///  Redirect user to the login page, informing that the link is used
    JwtRedirectLogin = 8006,
    AuthSwitchToSso = 8100,
    AuthSwitchToSrp = 8101,
    ///  Show the human verification modal as documented here: <https:confluence.protontech.ch/display/CP/Human+Verification>
    HumanVerificationRequired = 9001,
    ///  Prompt device verification as documented here: <https:confluence.protontech.ch/display/CP/Tech+Proposal%3A+Proof+of+work>
    DeviceVerificationRequired = 9002,
    ScopeMissingUnexpected = 9100,
    ScopeReauthLocked = 9101,
    ScopeReauthPassword = 9102,
    ScopeDelinquent = 9103,
    ScopeVerification = 9104,
    ScopeLoggedinCredentialless = 9105,
    ScopeUnauthSession = 9106,
    ///  Display an upsell modal after a free trial has expired
    TrialExpired = 9200,
    ///  Detect login failed
    AuthAuthAccountFailedGeneric = 10001,
    ///  Detect this account is disabled
    AuthAuthAccountDeleted = 10002,
    ///  Show the abuse fraud modal
    AuthAuthAccountDisabled = 10003,
    ///  Specific flow for upgrade
    AuthAuthPaidPlanRequired = 10004,
    ///  Hide the refresh token error
    AuthRefreshTokenInvalid = 10013,
    ///  Hide the refresh token error
    AuthCookiesRefreshInvalid = 10021,
    ///  Detect superfluous 2FA submission
    Auth2faNotEnabled = 10100,
    ///  Detect invalid 2FA
    Auth2faInputInvalid = 10101,
    ///  Detect invalid 2FA
    Auth2faTokenInvalid = 10102,
    ///  Could not create credential-less user
    AccountCredentiallessInvalid = 10200,
    ///  Specific device not found
    AuthDeviceNotFound = 10300,
    ///  Activate device using other devices or backup password
    AuthDeviceNotActive = 10301,
    ///  Delete local secret and create new device
    AuthDeviceTokenInvalid = 10302,
    ///  Delete local secret and create new device
    AuthDeviceRejected = 10303,
    ///  Set up user by creating keys or change the password (done by admin)
    MissingSrpParameters = 10400,
    UserUpdateEmailEmailInvalid = 12006,
    UserUpdate2faConfirmationFailed = 12060,
    UserUpdate2faInputInvalid = 12061,
    ///  Detect failed human verification
    UserCreateTokenInvalid = 12087,
    ///  Used for `Force2FA` and `ForcePasswordChange`
    UserRestrictedState = 12100,
    ///  Display custom please use a non-ProtonMail email address on signup
    UserCodeEmailInvalidProtonmail = 12220,
    ///  Display custom invalid email address
    UserCodeEmailInvalid = 12221,
    UserQuotaExceeded = 12403,
    ///  Handle the "Invalid email" error
    KeyGetInputInvalid = 33101,
    ///  Handle the "Invalid email" error
    KeyGetAddressMissing = 33102,
    ///  Handle the "Invalid email" error
    KeyGetDomainExternal = 33103,
    ///  Handle the "Invalid email" error
    KeyGetInvalidKt = 33104,
    ///  Display custom error on mnemonic ban on `WebClient`
    BansMnemonic = 85071,
    BansVerifyEmail = 85102,
}

pub enum Mail {
    ///  Create a new message when the user deletes a draft in another tab
    MessageUpdateDraftNotExist = 15033,
    ///  Close the composer where the message has already been sent, race condition?
    MessageUpdateDraftNotDraft = 15034,
    ///  Hide this error message
    MessageValidateKeyIdNotAssociated = 15213,
    ///  Show a custom search syntax error
    MessageSearchQuerySyntax = 15225,
    /// Mail Refresh the list
    IncomingDefaultUpdateNotExist = 35023,
    ///  Hanldle the message too large to import
    ImportMessageTooLarge = 36022,
    ///  Hide the create filter modal
    FilterCreateTooManyActive = 50016,
    /// This message was already sent
    MessageAlreadySent = 2500,
    /// Can no longer undo send
    MessageSentCanNoLongerBeUndone = 2511,
    /// Attachment Message already sent
    AttachmentMessageAlreadySent = 11109,
    /// Too Many Attachments
    TooManyAttachments = 2024,
    ExpirationTimeTooSoon = 2023,
    MessageDoesNotExist = 2501,
    ConversationDoesNotExist = 20052,
    StorageQuotaExceeded = 11100,
    AttachmentDoesNotExist = 11127,
    AttachmentMessageDoesNotExist = 11125,
    AttachmentMessageNotADraft = 11126,
}

pub enum Payments {
    ///  Respond to Apple payments issue
    PaymentsSubscriptionAmountMismatch = 22101,
    ///  Check if user has any existing subscriptions before allowing in app purchases
    PaymentsSubscriptionNotExists = 22110,
    ///  Create a waitlist
    PaymentsSubscriptionTooLittleSpace = 22117,
    PaymentsStripe3dsRequired = 22718,
    ///  Show a specific paypal network error
    PaymentsPaypalConnectionException = 22802,
    ///  Respond to Apple payments issue
    PaymentsAppleCurrencyInvalid = 22915,
}

pub enum Vpn {
    /// Handle invalid profile ID (when updating)
    VpnProfileUpdateIdInvalid = 86062,
    /// Handle invalid profile ID (when deleting)
    VpnProfileDeleteIdInvalid = 86063,
    /// Handle a profile with this name already exists (when creating or updating)
    VpnProfileNameAlreadyUsed = 86065,
    /// When the current action cannot be performed while connected to one of our servers
    VpnConnectionDisallowed = 86081,
    /// When key is not known from the DB
    VpnKeyNotFound = 86100,
    /// When trying to connect using an expired certificate
    VpnExpiredCertificate = 86101,
    /// When trying to connect using a revoked certificate
    VpnRevokedCertificate = 86102,
    /// When trying to connect on more than 1 server using 1 certificate
    VpnDuplicatedCertificate = 86103,
    /// When trying to connect restricted server or with a broken certificate
    VpnFailedCertificateVerification = 86104,
    /// When trying to connect with a certificate incorrectly signed, signed by an unknown, expired or revoked authority
    VpnFailedCertificateSignatureValidation = 86105,
    /// When trying to connect more devices than allowed simultaneously
    VpnSessionsCountOverQuota = 86110,
    /// When user has free plan and try to connect more devices than allowed simultaneously
    VpnSessionsCountOverQuotaFree = 86111,
    /// When user has basic plan and try to connect more devices than allowed simultaneously
    VpnSessionsCountOverQuotaBasic = 86112,
    /// When user has plus plan and try to connect more devices than allowed simultaneously
    VpnSessionsCountOverQuotaPlus = 86113,
    /// When user has visionary plan and try to connect more devices than allowed simultaneously
    VpnSessionsCountOverQuotaVisionary = 86114,
    /// When user has pro plan and try to connect more devices than allowed simultaneously
    VpnSessionsCountOverQuotaPro = 86115,
    /// Unknown error on the exit VPN server system
    VpnServerSystemError = 86150,
    /// When connected to a server not included in the user plan
    VpnServerNotAllowedWithCurrentPlan = 86151,
    /// When the plan that would allow to connect to the requested server has pending invoices
    VpnServerNotAllowedWhileInvoiceIsPending = 86152,
    /// When the user is torrenting while the plan does not allow it
    VpnTorrentNotAllowed = 86153,
    /// When the user has an abuser status
    VpnBadUserBehavior = 86154,
    /// When the user first need to assign VPN connections to use the client
    VpnNoConnectionAssigned = 86300,
    /// When the user needs to buy more dedicated IPs to continue
    VpnInsufficientDedicatedIps = 86301,
}

pub enum Drive {
    UnknownError = 200000,
    InsufficientQuota = 200001,
    InsufficientSpace = 200002,
    MaxFileSizeForFreeUser = 200003,
    InsufficientVolumeQuota = 200100,
    InsufficientDeviceQuota = 200101,
    AddressNotFound = 200200,
    AlreadyMemberOfShareInVolumeWithAnotherAddress = 200201,
    ShareAddressTypeGroupNotYetSupported = 200202,
    TooManyChildren = 200300,
    NestingTooDeep = 200301,
    ShareUrlCopyrightInfringement = 200400,
    EncryptionVerificationFailed = 200501,
    SignatureVerificationFailed = 200502,
    InsufficientInvitationQuota = 200600,
    InsufficientShareQuota = 200601,
    InsufficientShareJoinedQuota = 200602,
    RevisionNotEnabledForDocuments = 200700,
    FileCreationNotEnabledForDocuments = 200701,
    InsufficientBookmarksQuota = 200800,
    UserSettingsNotUpdated = 200900,
    UserSettingsNotEnabled = 200901,
}

pub enum Pass {
    NotLatestKey = 300001,
    NotLatestRevision = 300002,
    InvalidSignature = 300003,
    DeletedShare = 300004,
    RotationPayloadIncomplete = 300005,
    MissingKeys = 300006,
    ResourceLimitExceeded = 300007,
    SessionLocked = 300008,
}

pub enum KtBundle {
    SklFormat = 400001,
    VerifiedEpochNotFound = 400100,
    AddressListNotFound = 400101,
}
