use std::path::PathBuf;

use rstest::rstest;
use windows_sys::Win32::Security::{
    AddAccessDeniedAceEx, WinBuiltinAnyPackageSid, WinBuiltinGuestsSid, WinBuiltinUsersSid,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};

use super::*;
use crate::test_util::TempDir;

const OPEN: u32 = FILE_GENERIC_READ | FILE_GENERIC_WRITE;
const BOTH: u32 = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
const HERE: u32 = 0;
const FILES_ONLY: u32 = INHERIT_ONLY_ACE | OBJECT_INHERIT_ACE;
const FOLDERS_ONLY: u32 = INHERIT_ONLY_ACE | CONTAINER_INHERIT_ACE;
const ALLOW: bool = true;
const DENY: bool = false;

type Entry = (bool, WELL_KNOWN_SID_TYPE, u32, u32);

fn folder(tmp: &TempDir, name: &str) -> PathBuf {
    let dir = tmp.path().join(name);
    std::fs::create_dir(&dir).expect("folder");
    dir
}

fn shared(tmp: &TempDir, access: u32) -> PathBuf {
    let dir = folder(tmp, "shared");
    share_with_everyone(&dir, access).expect("shared");
    dir
}

fn read(dir: &Path) -> (u32, u32) {
    owner_and_mode(dir, &std::fs::symlink_metadata(dir).expect("metadata"))
}

fn with(dir: &Path, entries: &[Entry]) {
    let mut acl = Acl([0; ACL_WORDS]);
    ok(unsafe { InitializeAcl(acl.ptr(), bytes_of::<Acl>(), ACL_REVISION) }).expect("an ACL");
    ok(unsafe { AddAccessAllowedAceEx(acl.ptr(), ACL_REVISION, BOTH, FILE_ALL_ACCESS, me().ptr()) }).expect("mine");
    for &(allows, kind, access, flags) in entries {
        let add = if allows { AddAccessAllowedAceEx } else { AddAccessDeniedAceEx };
        ok(unsafe { add(acl.ptr(), ACL_REVISION, flags, access, sid(kind).ptr()) }).expect("an entry");
    }
    set_dacl(dir, &mut acl).expect("set the ACL");
}

fn sid(kind: WELL_KNOWN_SID_TYPE) -> Sid {
    Sid::well_known(kind).expect("a well-known SID")
}

fn me() -> Sid {
    user().expect("the user")
}

mod reading_a_folder {
    use super::*;

    #[test]
    fn in_the_profile_finds_it_private_to_the_user() {
        let tmp = TempDir::new();

        assert_eq!(read(&folder(&tmp, "dir")), (uid(), 0o700));
    }

    #[test]
    fn everyone_can_write_finds_it_open() {
        let tmp = TempDir::new();

        assert_eq!(read(&shared(&tmp, OPEN)), (uid(), 0o777));
    }

    #[test]
    fn everyone_can_read_finds_it_readable() {
        let tmp = TempDir::new();

        assert_eq!(read(&shared(&tmp, FILE_GENERIC_READ)), (uid(), 0o755));
    }

    #[rstest]
    #[case::another_group_that_can_write(&[(ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH)], 0o777)]
    #[case::system_administrators_and_app_packages(&[
        (ALLOW, WinLocalSystemSid, FILE_ALL_ACCESS, BOTH),
        (ALLOW, WinBuiltinAdministratorsSid, FILE_ALL_ACCESS, BOTH),
        (ALLOW, WinCreatorOwnerSid, FILE_ALL_ACCESS, BOTH),
        (ALLOW, WinBuiltinAnyPackageSid, FILE_ALL_ACCESS, BOTH),
    ], 0o700)]
    #[case::everyone_denied_before_a_group_is_allowed(&[
        (DENY, WinWorldSid, FILE_GENERIC_WRITE, BOTH),
        (ALLOW, WinBuiltinUsersSid, OPEN, BOTH),
    ], 0o755)]
    #[case::a_group_denied_before_it_is_allowed(&[
        (DENY, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
        (ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
    ], 0o700)]
    #[case::a_generic_deny_before_a_specific_allow(&[
        (DENY, WinWorldSid, GENERIC_WRITE, BOTH),
        (ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
    ], 0o700)]
    #[case::another_group_denied_before_a_group_is_allowed(&[
        (DENY, WinBuiltinGuestsSid, FILE_GENERIC_WRITE, BOTH),
        (ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
    ], 0o777)]
    #[case::a_deny_after_the_allow(&[
        (ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
        (DENY, WinWorldSid, FILE_GENERIC_WRITE, BOTH),
    ], 0o777)]
    #[case::a_deny_that_stops_at_the_folder(&[
        (DENY, WinWorldSid, FILE_GENERIC_WRITE, HERE),
        (ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
    ], 0o777)]
    #[case::a_deny_split_between_the_folder_and_its_files(&[
        (DENY, WinWorldSid, FILE_GENERIC_WRITE, HERE),
        (DENY, WinWorldSid, FILE_GENERIC_WRITE, FILES_ONLY),
        (ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
    ], 0o700)]
    #[case::a_deny_that_skips_the_folder(&[
        (DENY, WinWorldSid, FILE_GENERIC_WRITE, FILES_ONLY),
        (ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, BOTH),
    ], 0o777)]
    #[case::an_allow_for_files_only(&[(ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, FILES_ONLY)], 0o777)]
    #[case::an_allow_for_folders_inside_only(&[(ALLOW, WinBuiltinUsersSid, FILE_GENERIC_WRITE, FOLDERS_ONLY)], 0o700)]
    fn by_its_entries_in_order(#[case] entries: &[Entry], #[case] mode: u32) {
        let tmp = TempDir::new();
        let dir = folder(&tmp, "dir");

        with(&dir, entries);

        assert_eq!(read(&dir), (uid(), mode));
    }

    #[test]
    fn without_a_dacl_finds_it_open_to_everyone() {
        assert_eq!(others(&me(), null()), OTHERS_WRITE);
    }

    #[test]
    fn missing_reads_as_nobodys_and_open() {
        let tmp = TempDir::new();
        let missing = tmp.path().join("missing");
        let meta = std::fs::symlink_metadata(tmp.path()).expect("metadata");

        assert_eq!(owner_and_mode(&missing, &meta), (NOBODY, ANYONE_MAY_WRITE));
    }
}

mod making_a_folder_private {
    use super::*;

    #[test]
    fn takes_away_what_others_were_given() {
        let tmp = TempDir::new();
        let dir = shared(&tmp, OPEN);

        make_private(&dir).expect("made private");

        assert_eq!(read(&dir), (uid(), 0o700));
    }

    #[test]
    fn is_kept_by_what_is_made_inside() {
        let tmp = TempDir::new();
        let dir = shared(&tmp, OPEN);
        make_private(&dir).expect("made private");

        let inside = dir.join("inside");
        std::fs::create_dir(&inside).expect("a folder inside");

        assert_eq!(read(&inside), (uid(), 0o700));
    }

    #[test]
    fn of_a_missing_folder_fails() {
        let tmp = TempDir::new();

        assert!(make_private(&tmp.path().join("missing")).is_err());
    }
}

mod creating_private_folders {
    use super::*;

    #[test]
    fn makes_each_missing_one_private_inside_a_shared_folder() {
        let tmp = TempDir::new();
        let open = shared(&tmp, OPEN);
        let nested = open.join("a").join("b");

        create_private_dir_all(&nested).expect("created");

        assert_eq!(read(&open.join("a")), (uid(), 0o700));
        assert_eq!(read(&nested), (uid(), 0o700));
    }

    #[test]
    fn leaves_an_existing_folder_as_it_is() {
        let tmp = TempDir::new();
        let dir = shared(&tmp, FILE_GENERIC_READ);

        create_private_dir_all(&dir).expect("already there");

        assert_eq!(read(&dir), (uid(), 0o755));
    }

    #[test]
    fn refuses_a_file_in_the_way() {
        let tmp = TempDir::new();
        let file = tmp.path().join("file");
        std::fs::write(&file, "").expect("a file");

        assert!(create_private_dir_all(&file.join("dir")).is_err());
    }

    #[test]
    fn makes_the_user_its_owner() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("dir");

        create_private_dir_all(&dir).expect("created");

        let read = descriptor(&dir).expect("its descriptor");
        assert!(me().is(read.owner));
    }
}

mod a_process {
    use super::*;

    #[test]
    fn of_this_user_reads_as_theirs() {
        assert_eq!(process_uid(std::process::id()).expect("this process"), uid());
    }

    #[test]
    fn of_the_system_reads_as_nobodys() {
        assert_eq!(process_uid(4).expect("the System process"), NOBODY);
    }

    #[test]
    fn that_does_not_exist_cannot_be_read() {
        assert!(process_uid(u32::MAX - 3).is_err());
    }
}

#[test]
fn a_well_known_sid_is_found_only_as_itself() {
    let everyone = sid(WinWorldSid);

    assert!(known(everyone.ptr(), WinWorldSid));
    assert!(!known(everyone.ptr(), WinLocalSystemSid));
    assert!(!ours(&me(), everyone.ptr()));
    assert!(ours(&me(), me().ptr()));
}

#[rstest]
#[case::the_user(None, true)]
#[case::the_system(Some(WinLocalSystemSid), true)]
#[case::the_administrators(Some(WinBuiltinAdministratorsSid), true)]
#[case::everyone(Some(WinWorldSid), false)]
#[case::the_users_group(Some(WinBuiltinUsersSid), false)]
fn an_owner_is_ours_only_when_it_could_take_the_folder_anyway(
    #[case] kind: Option<WELL_KNOWN_SID_TYPE>,
    #[case] mine: bool,
) {
    let owner_sid = kind.map_or_else(me, sid);

    assert_eq!(owner(&me(), owner_sid.ptr()) == uid(), mine);
    assert_eq!(owner(&me(), null_mut()), NOBODY);
}
