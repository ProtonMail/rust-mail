pub trait DatabaseMarker: Send + Sync + Copy + std::fmt::Debug + Eq + 'static {}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AccountDb;

impl DatabaseMarker for AccountDb {}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct UserDb;

impl DatabaseMarker for UserDb {}
