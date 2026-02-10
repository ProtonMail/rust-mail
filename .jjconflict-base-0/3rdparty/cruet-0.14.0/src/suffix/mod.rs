/// Provides foreign key conversion for String.
///
/// Example string `foo` becomes `foo_id`
pub mod foreign_key;
pub use foreign_key::is_foreign_key;
pub use foreign_key::to_foreign_key;
