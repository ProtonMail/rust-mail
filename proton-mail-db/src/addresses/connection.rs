use crate::{DBResult, MailSqliteConnectionImpl};
use proton_api_mail::domain::{Address, AddressId, AddressSignedKeyList};
use proton_api_mail::exports::crypto::domain::{KeyFlag, KeyId, LockedKey};
use proton_api_mail::proton_api_core::exports::crypto::domain::AddressKeys;
use proton_sqlite3::rusqlite::{OptionalExtension, Row, Statement};
use proton_sqlite3::utils::mapped_rows_to_vec;

impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn create_or_update_address(&mut self, address: &Address) -> DBResult<()> {
        self.create_or_update_addresses(std::iter::once(address))
    }
    pub fn create_or_update_addresses<'i>(
        &mut self,
        addresses: impl Iterator<Item = &'i Address>,
    ) -> DBResult<()> {
        let mut address_stmt = self.0.prepare(
            "INSERT OR REPLACE INTO addresses \
(id, domain_id, email, send, receive, status, type, `order`, display_name, signature, catch_all, proton_mx, \
signed_key_list_min_epoch_id, signed_key_list_expected_min_epoch_id, signed_key_list_max_epoch_id, \
signed_key_list_data, signed_key_obsolescence_token, signed_key_signature, signed_key_revision) VALUES \
(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")?;

        let mut address_keys_stmt = self.0.prepare(
            "INSERT OR REPLACE INTO address_keys \
(id, address_id, version, private_key, token, signature, is_primary, is_active, flags, address_forwarding_id) \
VALUES (?,?,?,?,?,?,?,?,?,?);
            ")?;

        for addr in addresses {
            bind_address_to_create_or_update_statement(&mut address_stmt, addr)?;
            address_stmt.raw_execute()?;

            for key in addr.keys.as_ref() {
                execute_address_key_with_create_or_update_statement(
                    &mut address_keys_stmt,
                    addr,
                    key,
                )?;
            }
        }
        Ok(())
    }

    pub fn update_address(&mut self, address: &Address) -> DBResult<()> {
        self.update_addresses(std::iter::once(address))
    }
    pub fn update_addresses<'i>(
        &mut self,
        addresses: impl Iterator<Item = &'i Address>,
    ) -> DBResult<()> {
        let mut address_stmt = self.0.prepare(
            "UPDATE addresses SET \
domain_id=?2, email=?3, send=?4, receive=?5, status=?6, type=?7, `order`=?8, display_name=?9, \
signature=?10, catch_all=?11, proton_mx=?12, \
signed_key_list_min_epoch_id=?13, signed_key_list_expected_min_epoch_id=?14, signed_key_list_max_epoch_id=?15, \
signed_key_list_data=?16, signed_key_obsolescence_token=?17, signed_key_signature=?18, \
signed_key_revision=?19 WHERE id=?1")?;

        let mut address_keys_stmt = self.0.prepare(
            "INSERT OR REPLACE INTO address_keys \
(id, address_id, version, private_key, token, signature, is_primary, is_active, flags, address_forwarding_id) \
VALUES (?,?,?,?,?,?,?,?,?,?);
            ")?;
        //TODO: DO address updates also create new keys? If not use the statement below.
        /*let mut address_keys_stmt = self.0.prepare(
                    "UPDATE address_keys SET address_id=?2, version=?3, private_key=?4, token=?5, \
        signature=?6, is_primary=?7, is_active=?8, flags=?9, address_forwarding_id=?10 WHERE id=?1")?;*/

        for addr in addresses {
            bind_address_to_create_or_update_statement(&mut address_stmt, addr)?;
            address_stmt.raw_execute()?;

            for key in addr.keys.as_ref() {
                execute_address_key_with_create_or_update_statement(
                    &mut address_keys_stmt,
                    addr,
                    key,
                )?;
            }
        }
        Ok(())
    }

    pub fn get_address(&self, id: &AddressId) -> DBResult<Option<Address>> {
        let mut address = self
            .0
            .query_row(&AddressSelector::with_id(), [id], AddressSelector::from_row)
            .optional()?;
        if let Some(address) = &mut address {
            let mut key_stmt = self.0.prepare(&AddressKeySelector::with_address_id())?;
            let keys = mapped_rows_to_vec(
                key_stmt.query_map([&address.id], AddressKeySelector::from_row)?,
            )?;
            address.keys = AddressKeys(keys);
        }
        Ok(address)
    }

    pub fn delete_address(&mut self, id: &AddressId) -> DBResult<()> {
        self.delete_addresses(std::iter::once(id))
    }

    pub fn delete_addresses<'i>(
        &mut self,
        ids: impl Iterator<Item = &'i AddressId>,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare("DELETE FROM addresses WHERE id=?")?;
        for id in ids {
            stmt.execute([id])?;
        }
        Ok(())
    }
}

struct AddressSelector {}

impl AddressSelector {
    fn query() -> &'static str {
        "SELECT * FROM addresses"
    }

    fn with_id() -> String {
        format!("{} WHERE id=?", Self::query())
    }
    fn from_row(r: &Row) -> DBResult<Address> {
        Ok(Address {
            id: r.get(0)?,
            domain_id: r.get(1)?,
            email: r.get(2)?,
            send: r.get(3)?,
            receive: r.get(4)?,
            status: r.get(5)?,
            address_type: r.get(6)?,
            order: r.get(7)?,
            display_name: r.get(8)?,
            signature: r.get(9)?,
            keys: AddressKeys::new([]),
            catch_all: r.get(10)?,
            proton_mx: r.get(11)?,
            signed_key_list: AddressSignedKeyList {
                min_epoch_id: r.get(12)?,
                expected_min_epoch_id: r.get(13)?,
                max_epoch_id: r.get(14)?,
                data: r.get(15)?,
                obsolescence_token: r.get(16)?,
                signature: r.get(17)?,
                revision: r.get(18)?,
            },
        })
    }
}

struct AddressKeySelector {}
impl AddressKeySelector {
    fn query() -> &'static str {
        "SELECT id, private_key, token, signature, is_primary, is_active, flags FROM address_keys"
    }

    fn with_address_id() -> String {
        format!("{} WHERE address_id=?", Self::query())
    }
    fn from_row(r: &Row) -> DBResult<LockedKey> {
        //TODO: Forwarding Address Id
        Ok(LockedKey {
            id: KeyId::from(r.get::<usize, String>(0)?),
            version: 0,
            private_key: r.get(1)?,
            token: r.get(2)?,
            signature: r.get(3)?,
            activation: None,
            primary: r.get(4)?,
            active: r.get(5)?,
            flags: r.get::<usize, Option<u32>>(6)?.map(KeyFlag::from),
            recovery_secret: None,
            recovery_secret_signature: None,
            address_forwarding_id: None,
        })
    }
}

fn bind_address_to_create_or_update_statement(
    address_stmt: &mut Statement,
    addr: &Address,
) -> DBResult<()> {
    // we need to manually bind as rusqlite doesn't have this many convenience wrappers.
    address_stmt.raw_bind_parameter(1, &addr.id)?;
    address_stmt.raw_bind_parameter(2, &addr.domain_id)?;
    address_stmt.raw_bind_parameter(3, &addr.email)?;
    address_stmt.raw_bind_parameter(4, addr.send)?;
    address_stmt.raw_bind_parameter(5, addr.receive)?;
    address_stmt.raw_bind_parameter(6, addr.status)?;
    address_stmt.raw_bind_parameter(7, addr.address_type)?;
    address_stmt.raw_bind_parameter(8, addr.order)?;
    address_stmt.raw_bind_parameter(9, &addr.display_name)?;
    address_stmt.raw_bind_parameter(10, &addr.signature)?;
    address_stmt.raw_bind_parameter(11, addr.catch_all)?;
    address_stmt.raw_bind_parameter(12, addr.proton_mx)?;
    address_stmt.raw_bind_parameter(13, addr.signed_key_list.min_epoch_id)?;
    address_stmt.raw_bind_parameter(14, addr.signed_key_list.expected_min_epoch_id)?;
    address_stmt.raw_bind_parameter(15, addr.signed_key_list.max_epoch_id)?;
    address_stmt.raw_bind_parameter(16, &addr.signed_key_list.data)?;
    address_stmt.raw_bind_parameter(17, &addr.signed_key_list.obsolescence_token)?;
    address_stmt.raw_bind_parameter(18, &addr.signed_key_list.signature)?;
    address_stmt.raw_bind_parameter(19, addr.signed_key_list.revision)?;
    Ok(())
}

fn execute_address_key_with_create_or_update_statement(
    address_keys_stmt: &mut Statement,
    addr: &Address,
    key: &LockedKey,
) -> DBResult<usize> {
    //TODO: Address forwarding id
    address_keys_stmt.execute((
        key.id.as_ref(),
        &addr.id,
        3,
        &key.private_key,
        &key.token,
        &key.signature,
        key.primary,
        key.active,
        key.flags.map(|v| v.to_u32()),
        None::<AddressId>,
    ))
}
