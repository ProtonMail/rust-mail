pub trait DatabaseMarker: Send + Sync + Copy + 'static {}

/// Do not use yet: Context does not provide [`Stash<AccountDb>`]
/// Using this marker before the infrastructure is ready will cause type mismatches.
/// Use [`DefaultDb`] for now.
#[deprecated(
    note = "Not ready for use. Context does not provide Stash<AccountDb> yet. Use DefaultDb."
)]
#[derive(Copy, Clone)]
pub struct AccountDb;

#[allow(deprecated)]
impl DatabaseMarker for AccountDb {}

/// Do not use yet: Context does not provide [`Stash<UserDb>`]
/// Using this marker before the infrastructure is ready will cause type mismatches.
/// Use [`DefaultDb`] for now.
#[deprecated(
    note = "Not ready for use. Context does not provide Stash<UserDb> yet. Use DefaultDb."
)]
#[derive(Copy, Clone)]
pub struct UserDb;

#[allow(deprecated)]
impl DatabaseMarker for UserDb {}

/// Temporary backward-compatibility marker that will be removed in a future phase.
#[derive(Copy, Clone)]
pub struct DefaultDb;
impl DatabaseMarker for DefaultDb {}
