pub trait DatabaseMarker: Send + Sync + Copy + 'static {}

#[derive(Copy, Clone)]
pub struct AccountDb;

impl DatabaseMarker for AccountDb {}

#[derive(Copy, Clone)]
pub struct UserDb;

impl DatabaseMarker for UserDb {}
