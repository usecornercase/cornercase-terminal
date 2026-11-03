#![cfg(windows)]

use std::fs::File;
use std::os::windows::io::{AsRawHandle, OwnedHandle, RawHandle};

use super::imp::Taken;
use crate::test_util::TempDir;

fn handles(tmp: &TempDir, count: usize) -> Vec<OwnedHandle> {
    (0..count).map(|n| OwnedHandle::from(File::create(tmp.path().join(format!("log{n}"))).expect("file"))).collect()
}

mod the_log_taking_stderr {
    use super::*;

    #[test]
    fn keeps_the_last_two_handles_and_closes_the_one_before() {
        let tmp = TempDir::new();
        let taken = handles(&tmp, 4);
        let raw: Vec<RawHandle> = taken.iter().map(AsRawHandle::as_raw_handle).collect();
        let mut kept = Taken::new();

        let closed: Vec<Option<RawHandle>> =
            taken.into_iter().map(|handle| kept.take(handle).map(|old| old.as_raw_handle())).collect();

        assert_eq!(closed, [None, None, Some(raw[0]), Some(raw[1])]);
    }
}
