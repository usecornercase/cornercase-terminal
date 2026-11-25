use std::io;

pub(crate) use imp::read;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Memory {
    pub(crate) total: u64,
    pub(crate) available: u64,
}

#[cfg(any(target_os = "linux", test))]
fn meminfo(text: &str) -> Option<Memory> {
    let field = |name: &str| {
        let line = text.lines().find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))?;
        let mut words = line.split_whitespace();
        let amount: u64 = words.next()?.parse().ok()?;
        match words.next() {
            Some("kB") => amount.checked_mul(1024),
            None => Some(amount),
            Some(_) => None,
        }
    };
    let total = field("MemTotal")?;
    let available = field("MemAvailable").or_else(|| {
        let free = field("MemFree")?;
        Some(free.saturating_add(field("Buffers").unwrap_or(0)).saturating_add(field("Cached").unwrap_or(0)))
    })?;
    (total > 0).then_some(Memory { total, available: available.min(total) })
}

#[cfg(any(target_os = "macos", test))]
fn from_pages(total: u64, pages: [u64; 3], page: u64) -> Option<Memory> {
    let available = pages.iter().fold(0_u64, |sum, count| sum.saturating_add(count.saturating_mul(page)));
    (total > 0 && page > 0).then_some(Memory { total, available: available.min(total) })
}

fn unread(what: &str) -> io::Error {
    io::Error::other(format!("the system's memory could not be read: {what}"))
}

#[cfg(target_os = "linux")]
mod imp {
    use std::io;

    use super::{Memory, meminfo, unread};

    pub(crate) fn read() -> io::Result<Memory> {
        meminfo(&std::fs::read_to_string("/proc/meminfo")?).ok_or_else(|| unread("/proc/meminfo has no MemTotal"))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, c_void};
    use std::io;
    use std::mem::size_of;
    use std::ptr;

    use super::{Memory, from_pages, unread};

    unsafe extern "C" {
        fn mach_host_self() -> libc::mach_port_t;
    }

    fn total() -> io::Result<u64> {
        let name: &CStr = c"hw.memsize";
        let mut total: u64 = 0;
        let mut size = size_of::<u64>();
        let read = unsafe {
            libc::sysctlbyname(name.as_ptr(), (&raw mut total).cast::<c_void>(), &raw mut size, ptr::null_mut(), 0)
        };
        if read != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(total)
    }

    fn statistics() -> io::Result<libc::vm_statistics64> {
        let mut info: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        let read = unsafe {
            libc::host_statistics64(
                mach_host_self(),
                libc::HOST_VM_INFO64,
                (&raw mut info).cast::<libc::integer_t>(),
                &raw mut count,
            )
        };
        if read != libc::KERN_SUCCESS {
            return Err(unread(&format!("host_statistics64 answered {read}")));
        }
        Ok(info)
    }

    pub(crate) fn read() -> io::Result<Memory> {
        let info = statistics()?;
        let page = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).unwrap_or(0);
        let pages = [u64::from(info.free_count), u64::from(info.inactive_count), u64::from(info.purgeable_count)];
        from_pages(total()?, pages, page).ok_or_else(|| unread("no total or page size"))
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::mem::size_of;

    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    use super::{Memory, unread};

    pub(crate) fn read() -> io::Result<Memory> {
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(u32::MAX);
        if unsafe { GlobalMemoryStatusEx(&raw mut status) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if status.ullTotalPhys == 0 {
            return Err(unread("GlobalMemoryStatusEx gave no total"));
        }
        Ok(Memory { total: status.ullTotalPhys, available: status.ullAvailPhys.min(status.ullTotalPhys) })
    }
}

#[cfg(test)]
mod tests;
