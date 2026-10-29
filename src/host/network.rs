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

#[cfg(test)]
mod tests;
