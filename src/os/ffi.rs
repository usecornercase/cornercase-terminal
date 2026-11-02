pub(crate) use imp::{OsStrExt, OsStringExt};

#[cfg(unix)]
mod imp {
    pub(crate) use std::os::unix::ffi::{OsStrExt, OsStringExt};
}

#[cfg(not(unix))]
mod imp {
    use std::ffi::{OsStr, OsString};

    pub(crate) trait OsStrExt {
        fn from_bytes(slice: &[u8]) -> &Self;
        fn as_bytes(&self) -> &[u8];
    }

    impl OsStrExt for OsStr {
        fn from_bytes(slice: &[u8]) -> &Self {
            Self::new(std::str::from_utf8(slice).unwrap_or_default())
        }

        fn as_bytes(&self) -> &[u8] {
            self.as_encoded_bytes()
        }
    }

    pub(crate) trait OsStringExt {
        fn from_vec(vec: Vec<u8>) -> Self;
        fn into_vec(self) -> Vec<u8>;
    }

    impl OsStringExt for OsString {
        fn from_vec(vec: Vec<u8>) -> Self {
            String::from_utf8(vec).map(Self::from).unwrap_or_default()
        }

        fn into_vec(self) -> Vec<u8> {
            self.into_encoded_bytes()
        }
    }
}
