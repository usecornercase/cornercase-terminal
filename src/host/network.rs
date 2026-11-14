use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};

const TEST_NET_4: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);
const TEST_NET_6: Ipv6Addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
const DISCARD: u16 = 9;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Route {
    pub(crate) v4: Option<IpAddr>,
    pub(crate) v6: Option<IpAddr>,
}

pub(crate) fn default_route() -> Route {
    Route {
        v4: source(IpAddr::V4(Ipv4Addr::UNSPECIFIED), IpAddr::V4(TEST_NET_4)),
        v6: source(IpAddr::V6(Ipv6Addr::UNSPECIFIED), IpAddr::V6(TEST_NET_6)),
    }
}

fn source(any: IpAddr, far: IpAddr) -> Option<IpAddr> {
    let socket = UdpSocket::bind(SocketAddr::new(any, 0)).ok()?;
    socket.connect(SocketAddr::new(far, DISCARD)).ok()?;
    socket.local_addr().ok().map(|local| local.ip()).filter(|ip| !ip.is_unspecified())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reach {
    Lan,
    Tailscale,
}

const TAILSCALE_V6: [u16; 3] = [0xfd7a, 0x115c, 0xa1e0];

fn eligible(ip: IpAddr) -> Option<Reach> {
    match ip {
        IpAddr::V4(v4) if v4.is_private() => Some(Reach::Lan),
        IpAddr::V4(v4) if v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]) => Some(Reach::Tailscale),
        IpAddr::V6(v6) if v6.segments()[..3] == TAILSCALE_V6 => Some(Reach::Tailscale),
        _ => None,
    }
}

fn sorted(mut ips: Vec<IpAddr>) -> Vec<IpAddr> {
    ips.retain(|ip| eligible(*ip).is_some());
    ips.sort_by_key(|ip| (eligible(*ip), *ip));
    ips.dedup();
    ips
}

pub(crate) fn addresses() -> Vec<IpAddr> {
    let found =
        if_addrs::get_if_addrs().map(|all| all.iter().map(if_addrs::Interface::ip).collect()).unwrap_or_default();
    sorted(found)
}

pub(crate) fn listed(text: &str) -> Vec<IpAddr> {
    let mut seen = Vec::new();
    for ip in text.split(',').filter_map(|part| part.trim().parse::<IpAddr>().ok()) {
        if !seen.contains(&ip) {
            seen.push(ip);
        }
    }
    seen
}

#[cfg(test)]
mod tests;
