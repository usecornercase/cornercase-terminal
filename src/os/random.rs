pub(crate) use imp::fill;

#[cfg(unix)]
mod imp {
    use std::io::{self, Read};

    pub(crate) fn fill(buf: &mut [u8]) -> io::Result<()> {
        std::fs::File::open("/dev/urandom")?.read_exact(buf)
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::ptr;

    use windows_sys::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};

    pub(crate) fn fill(buf: &mut [u8]) -> io::Result<()> {
        let len = u32::try_from(buf.len()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let status =
            unsafe { BCryptGenRandom(ptr::null_mut(), buf.as_mut_ptr(), len, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
        if status < 0 {
            return Err(io::Error::other(format!("the system's random numbers failed ({status:#x})")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
