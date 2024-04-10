use crate::db::{DBResult, MailSqliteConnectionMut};
use proton_api_mail::domain::Address;

pub fn handle_address_event(
    tx: &mut MailSqliteConnectionMut,
    addresses: &[Address],
) -> DBResult<()> {
    tx.create_or_update_addresses(addresses.iter())
}
