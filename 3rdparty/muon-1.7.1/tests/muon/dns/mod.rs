use anyhow::Result;
use muon::common::{Addr, Host, IntoDyn};
use muon::dns::{CloudflareDns, DnsClient, DnsResolver, DynDnsService, GoogleDns, Quad9Dns};
use muon::rt::{AsyncResolver, AsyncSpawner, Resolver};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use test_case::test_case;

#[test_case(GoogleDns)]
#[test_case(CloudflareDns)]
#[test_case(Quad9Dns)]
#[tokio::test]
async fn test_dns_client(dns: impl IntoDyn<DynDnsService>) -> Result<()> {
    let host = Host::direct("ip.me")?;
    let want = AsyncResolver.resolve(&host).await?.into_set();
    assert!(want.has_ipv4() || want.has_ipv6());

    let dns = DnsClient::new(dns, AsyncSpawner::default());
    let have = DnsResolver::new(dns).resolve(&host).await?.into_set();
    assert!(have.has_ipv4() || have.has_ipv6());

    if want.has_ipv4() && have.has_ipv4() {
        assert_eq!(have.get_ipv4(), want.get_ipv4());
    }

    if want.has_ipv6() && have.has_ipv6() {
        assert_eq!(have.get_ipv6(), want.get_ipv6());
    }

    Ok(())
}

trait HashSetExt {
    fn has_ipv4(&self) -> bool;
    fn get_ipv4(&self) -> HashSet<Ipv4Addr>;

    fn get_ipv6(&self) -> HashSet<Ipv6Addr>;
    fn has_ipv6(&self) -> bool;
}

impl HashSetExt for HashSet<Addr> {
    fn has_ipv4(&self) -> bool {
        !self.get_ipv4().is_empty()
    }

    fn get_ipv4(&self) -> HashSet<Ipv4Addr> {
        let mut set = HashSet::new();

        for addr in self {
            if let IpAddr::V4(addr) = addr.ip {
                set.insert(addr);
            }
        }

        set
    }

    fn has_ipv6(&self) -> bool {
        !self.get_ipv6().is_empty()
    }

    fn get_ipv6(&self) -> HashSet<Ipv6Addr> {
        let mut set = HashSet::new();

        for addr in self {
            if let IpAddr::V6(addr) = addr.ip {
                set.insert(addr);
            }
        }

        set
    }
}
