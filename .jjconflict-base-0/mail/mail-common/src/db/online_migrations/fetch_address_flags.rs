use crate::{MailContextError, MailContextResult, MailUserContext};
use proton_core_api::services::proton::{AddressId, ProtonCore};
use proton_core_common::models::{Address, ModelExtension};
use stash::orm::Model;
use std::{sync::Weak, time::Duration};
use tokio::time;
use tracing::{debug, info, instrument};

#[instrument(skip_all)]
pub async fn run(ctx: &Weak<MailUserContext>) -> MailContextResult<()> {
    for address in get_addresses(ctx).await? {
        if let Some(address_id) = address.remote_id.clone() {
            update_address(ctx, address, address_id).await?;
        }
    }

    Ok(())
}

#[instrument(skip_all)]
async fn get_addresses(ctx: &Weak<MailUserContext>) -> MailContextResult<Vec<Address>> {
    let tether = ctx
        .upgrade()
        .ok_or(MailContextError::LostContext)?
        .user_stash()
        .connection()
        .await?;

    let addresses = Address::find("WHERE flags IS NULL", Vec::new(), &tether).await?;

    info!("Found {} address(es) to update", addresses.len());

    Ok(addresses)
}

#[instrument(skip_all, fields(id = ?address_id))]
async fn update_address(
    ctx: &Weak<MailUserContext>,
    mut address: Address,
    address_id: AddressId,
) -> MailContextResult<()> {
    info!("Fetching address from API");

    let api_address = loop {
        let ctx = ctx.upgrade().ok_or(MailContextError::LostContext)?;

        match ctx.session().get_address_by_id(address_id.clone()).await {
            Ok(address) => {
                break address;
            }

            Err(err) => {
                if err.is_network_failure() {
                    debug!(
                        "Couldn't fetch address from the API, got a network \
                         failure - waiting until we're online",
                    );

                    let mut network = ctx.network_monitor_service().os_network_status_observer();

                    // We're a background task, so let's not keep the context
                    // artificially alive when not needed
                    drop(ctx);

                    network.wait_until_online().await;

                    // Just for safety's sake - in case we're online, but still
                    // get network failures
                    time::sleep(Duration::from_secs(1)).await;

                    continue;
                } else if err.is_server_failure() {
                    debug!(
                        "Couldn't fetch address from the API, got a server \
                         failure - will try again in a moment",
                    );

                    // We're a background task, so let's not keep the context
                    // artificially alive when not needed
                    drop(ctx);

                    time::sleep(Duration::from_secs(60)).await;
                    continue;
                } else {
                    return Err(err.into());
                }
            }
        }
    };

    info!("Updating address in local database");

    let mut tether = ctx
        .upgrade()
        .ok_or(MailContextError::LostContext)?
        .user_stash()
        .connection()
        .await?;

    tether
        .tx(async |bond| {
            address.reload(bond).await?;
            address.flags = Some(api_address.address.flags.into());
            address.save(bond).await
        })
        .await?;

    info!("Address updated");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::MailTestContext;
    use proton_core_api::services::proton::{
        Address as ApiAddress, AddressFlags as ApiAddressFlags,
    };
    use proton_core_common::{datatypes::AddressFlags, test_utils::addresses::ApiAddressTestUtils};

    #[tokio::test]
    async fn smoke() {
        let ctx = MailTestContext::new().await;
        let muctx = ctx.uninitialized_mail_user_context().await;

        // ---

        let mut addr1 = {
            let mut addr = Address::from(ApiAddress::test_address());

            addr.remote_id = Some("1".into());
            addr.email = "one@proton.me".into();
            addr.flags = Some(AddressFlags(123));
            addr
        };

        let mut addr2 = {
            let mut addr = Address::from(ApiAddress::test_address());

            addr.remote_id = Some("2".into());
            addr.email = "two@proton.me".into();
            addr.flags = None;
            addr
        };

        let mut tether = muctx.user_stash().connection().await.unwrap();

        tether
            .tx(async |bond| {
                addr1.save(bond).await?;
                addr2.save(bond).await
            })
            .await
            .unwrap();

        ctx.core_test_context
            .mock_get_address(ApiAddress {
                id: "2".into(),
                flags: ApiAddressFlags(456),
                ..ApiAddress::test_address()
            })
            .await;

        // ---

        run(&muctx.as_weak()).await.unwrap();

        // ---

        addr1.reload(&tether).await.unwrap();
        addr2.reload(&tether).await.unwrap();

        assert_eq!(Some(AddressFlags(123)), addr1.flags);
        assert_eq!(Some(AddressFlags(456)), addr2.flags);
    }
}
