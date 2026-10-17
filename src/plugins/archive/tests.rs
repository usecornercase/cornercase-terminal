use std::io::Write as _;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::*;
use crate::test_util::TempDir;
use crate::text::hex;

fn gzip(tar: &[u8]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(tar).expect("gzip");
    gz.finish().expect("gzip")
}

fn raw(name: &str, kind: EntryType, link: Option<&str>, body: &[u8]) -> Vec<u8> {
    let mut header = tar::Header::new_old();
    let bytes = name.as_bytes();
    header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
    if let Some(link) = link {
        header.as_old_mut().linkname[..link.len()].copy_from_slice(link.as_bytes());
    }
    header.set_entry_type(kind);
    header.set_mode(0o4755);
    header.set_size(body.len() as u64);
    header.set_cksum();
    let mut out = header.as_bytes().to_vec();
    out.extend_from_slice(body);
    out.resize(out.len().div_ceil(512) * 512, 0);
    out
}

fn archive(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut tar: Vec<u8> = entries.concat();
    tar.extend(std::iter::repeat_n(0, 1024));
    gzip(&tar)
}

#[test]
fn files_and_folders_are_unpacked_without_setuid() {
    let dir = TempDir::new();
    let bytes =
        archive(&[raw("bin/", EntryType::Directory, None, b""), raw("bin/tool", EntryType::Regular, None, b"hi")]);
    assert_eq!(unpack(&bytes, dir.path()), Ok(2));
    let mode = fs::metadata(dir.path().join("bin/tool")).expect("tool").permissions();
    assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode) & 0o7777, 0o755);
}

#[rstest::rstest]
#[case::a_parent_folder("../escape", EntryType::Regular, None)]
#[case::an_absolute_path("/tmp/escape", EntryType::Regular, None)]
#[case::a_symbolic_link("link", EntryType::Symlink, Some("/etc"))]
#[case::a_hard_link("link", EntryType::Link, Some("/etc/passwd"))]
#[case::a_fifo("pipe", EntryType::Fifo, None)]
fn an_archive_holding_this_is_refused(#[case] name: &str, #[case] kind: EntryType, #[case] link: Option<&str>) {
    let dir = TempDir::new();
    let inner = dir.path().join("inner");
    fs::create_dir(&inner).expect("inner");
    assert!(unpack(&archive(&[raw(name, kind, link, b"")]), &inner).is_err());
    assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 1);
    assert_eq!(fs::read_dir(&inner).expect("list").count(), 0);
}

#[test]
fn a_file_is_never_written_through_a_link_in_the_checkout() {
    let (dir, outside) = (TempDir::new(), TempDir::new());
    std::os::unix::fs::symlink(outside.path(), dir.path().join("bin")).expect("symlink");
    let error = unpack(&archive(&[raw("bin/tool", EntryType::Regular, None, b"hi")]), dir.path()).expect_err("refused");
    assert!(error.contains("symbolic link"), "{error}");
    assert_eq!(fs::read_dir(outside.path()).expect("list").count(), 0);
}

#[test]
fn a_file_that_replaces_a_link_does_not_follow_it() {
    let (dir, outside) = (TempDir::new(), TempDir::new());
    let target = outside.path().join("kept");
    fs::write(&target, "mine").expect("write");
    std::os::unix::fs::symlink(&target, dir.path().join("tool")).expect("symlink");
    unpack(&archive(&[raw("tool", EntryType::Regular, None, b"new")]), dir.path()).expect("unpack");
    assert_eq!(fs::read_to_string(&target).expect("kept"), "mine");
}

#[test]
fn the_sha256_must_match() {
    let sum = hex(&Sha256::digest(b"abc"));
    assert!(verify(b"abc", &sum.to_uppercase()).is_ok());
    assert!(verify(b"abd", &sum).is_err());
}
