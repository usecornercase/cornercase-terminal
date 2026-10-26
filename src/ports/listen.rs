use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub(crate) use imp::listeners;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listener {
    pub(crate) pid: i32,
    pub(crate) port: u16,
    pub(crate) address: IpAddr,
}

#[cfg(any(target_os = "linux", test))]
const LISTEN: &str = "0A";

#[cfg(any(target_os = "linux", test))]
fn listening_sockets(table: &str) -> Vec<(u64, IpAddr, u16)> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.get(3) != Some(&LISTEN) {
                return None;
            }
            let (address, port) = fields.get(1)?.split_once(':')?;
            let inode = fields.get(9)?.parse().ok()?;
            Some((inode, hex_address(address)?, u16::from_str_radix(port, 16).ok()?))
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn hex_address(hex: &str) -> Option<IpAddr> {
    let words = (0..hex.len() / 8)
        .map(|i| hex.get(i * 8..i * 8 + 8).and_then(|word| u32::from_str_radix(word, 16).ok()))
        .collect::<Option<Vec<u32>>>()?;
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_ne_bytes()).collect();
    match bytes.len() {
        4 => <[u8; 4]>::try_from(bytes).ok().map(|b| IpAddr::V4(Ipv4Addr::from(b))),
        16 => <[u8; 16]>::try_from(bytes).ok().map(|b| IpAddr::V6(Ipv6Addr::from(b)).to_canonical()),
        _ => None,
    }
}

#[cfg(any(target_os = "macos", test))]
mod socket_fdinfo {
    pub(super) const SIZE: usize = 792;
    pub(super) const KIND: usize = 256;
    pub(super) const PORT: usize = 268;
    pub(super) const FAMILIES: usize = 288;
    pub(super) const ADDRESS: usize = 312;
    pub(super) const STATE: usize = 344;
    pub(super) const TCP: i32 = 2;
    pub(super) const LISTENING: i32 = 1;
    pub(super) const IPV6: u8 = 2;
}

#[cfg(any(target_os = "macos", test))]
fn tcp_listener(info: &[u8]) -> Option<(IpAddr, u16)> {
    use socket_fdinfo::{ADDRESS, FAMILIES, IPV6, KIND, LISTENING, PORT, SIZE, STATE, TCP};
    let int = |at: usize| info.get(at..at + 4)?.try_into().ok().map(i32::from_ne_bytes);
    if info.len() != SIZE || int(KIND)? != TCP || int(STATE)? != LISTENING {
        return None;
    }
    let port = u16::from_be_bytes(info.get(PORT..PORT + 2)?.try_into().ok()?);
    let address: [u8; 16] = info.get(ADDRESS..ADDRESS + 16)?.try_into().ok()?;
    let address = if info.get(FAMILIES)? & IPV6 == 0 {
        IpAddr::V4(Ipv4Addr::new(address[12], address[13], address[14], address[15]))
    } else {
        IpAddr::V6(Ipv6Addr::from(address)).to_canonical()
    };
    Some((address, port))
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::HashMap;
    use std::net::IpAddr;

    use super::{Listener, listening_sockets};

    pub(crate) fn listeners(pids: &[i32]) -> Vec<Listener> {
        let sockets: HashMap<u64, (IpAddr, u16)> = ["/proc/net/tcp", "/proc/net/tcp6"]
            .into_iter()
            .filter_map(|table| std::fs::read_to_string(table).ok())
            .flat_map(|table| listening_sockets(&table))
            .map(|(inode, address, port)| (inode, (address, port)))
            .collect();
        if sockets.is_empty() {
            return Vec::new();
        }
        pids.iter().flat_map(|&pid| of(pid, &sockets)).collect()
    }

    fn of(pid: i32, sockets: &HashMap<u64, (IpAddr, u16)>) -> Vec<Listener> {
        crate::process::open_files(pid)
            .iter()
            .filter_map(|link| link.to_str()?.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok())
            .filter_map(|inode: u64| sockets.get(&inode))
            .map(|&(address, port)| Listener { pid, port, address })
            .collect()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_int;
    use std::ptr::null_mut;

    use super::{Listener, socket_fdinfo, tcp_listener};

    const PROC_PIDFDSOCKETINFO: c_int = 3;

    pub(crate) fn listeners(pids: &[i32]) -> Vec<Listener> {
        pids.iter().flat_map(|&pid| of(pid)).collect()
    }

    fn of(pid: i32) -> Vec<Listener> {
        descriptors(pid)
            .iter()
            .filter(|fd| u32::try_from(libc::PROX_FDTYPE_SOCKET).is_ok_and(|socket| fd.proc_fdtype == socket))
            .filter_map(|fd| socket(pid, fd.proc_fd))
            .map(|(address, port)| Listener { pid, port, address })
            .collect()
    }

    fn descriptors(pid: i32) -> Vec<libc::proc_fdinfo> {
        let size = size_of::<libc::proc_fdinfo>();
        let needed = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, null_mut(), 0) };
        let Some(count) = usize::try_from(needed).ok().filter(|n| *n > 0).map(|n| n / size + 16) else {
            return Vec::new();
        };
        let mut fds: Vec<libc::proc_fdinfo> = vec![unsafe { std::mem::zeroed() }; count];
        let bytes = c_int::try_from(count * size).unwrap_or(0);
        let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, fds.as_mut_ptr().cast(), bytes) };
        fds.truncate(usize::try_from(read).unwrap_or(0) / size);
        fds
    }

    fn socket(pid: i32, fd: i32) -> Option<(std::net::IpAddr, u16)> {
        let mut info = [0_u8; socket_fdinfo::SIZE];
        let size = c_int::try_from(info.len()).ok()?;
        let read = unsafe { libc::proc_pidfdinfo(pid, fd, PROC_PIDFDSOCKETINFO, info.as_mut_ptr().cast(), size) };
        (read == size).then(|| tcp_listener(&info))?
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use super::Listener;

    pub(crate) fn listeners(_pids: &[i32]) -> Vec<Listener> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
