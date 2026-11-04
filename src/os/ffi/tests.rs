use std::path::{Path, PathBuf};

use rstest::rstest;

use super::*;

#[rstest]
#[case::a_file("src/main.rs")]
#[case::a_name_with_spaces("a folder/a file.txt")]
#[case::a_name_out_of_ascii("año/ñandú.txt")]
fn a_relative_path_joins_onto_its_folder(#[case] relative: &str) {
    let dir = std::env::temp_dir();
    let joined = join(&dir, relative);
    assert_eq!(joined, dir.join(relative.split('/').collect::<PathBuf>()));
    assert_eq!(text(joined.strip_prefix(&dir).expect("inside the folder")), relative);
}

#[test]
fn a_path_goes_to_bytes_and_back() {
    let path = std::env::temp_dir().join("año").join("ñandú.txt");
    assert_eq!(to_path(&to_bytes(&path)), path);
}

#[test]
fn the_same_path_is_the_same() {
    let dir = std::env::temp_dir();
    assert!(same(&dir, &dir.join("")));
    assert!(!same(&dir.join("a"), &dir.join("b")));
}

#[cfg(unix)]
mod on_unix {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    use super::*;

    #[test]
    fn bytes_that_are_not_utf_8_stay_as_they_are() {
        let bytes = b"repo/caf\xe9.txt";
        let path = to_path(bytes);
        assert_eq!(path.as_os_str().as_bytes(), bytes);
        assert_eq!(&*to_bytes(&path), bytes);
        assert_eq!(join(Path::new("/r"), "a/b"), Path::new("/r/a/b"));
        assert_eq!(to_os(bytes.to_vec()).as_os_str(), OsStr::from_bytes(bytes));
    }

    #[test]
    fn a_path_list_is_split_on_colons() {
        assert_eq!(LIST_SEPARATOR, b':');
    }
}

#[cfg(not(unix))]
mod on_windows {
    use super::*;

    #[rstest]
    #[case::a_drive("C:/Users/me/repo", r"C:\Users\me\repo")]
    #[case::a_relative_path("src/a b/c.rs", r"src\a b\c.rs")]
    #[case::a_name_out_of_ascii("C:/Users/José/año/ñandú.txt", r"C:\Users\José\año\ñandú.txt")]
    #[case::a_share("//server/share/repo", r"\\server\share\repo")]
    fn git_s_slashes_become_backslashes(#[case] printed: &str, #[case] native: &str) {
        assert_eq!(to_path(printed.as_bytes()).as_os_str(), native);
    }

    #[test]
    fn bytes_that_are_not_utf_8_are_replaced() {
        assert_eq!(to_path(b"caf\xe9.txt").as_os_str(), "caf\u{fffd}.txt");
        assert_eq!(to_os(b"caf\xe9".to_vec()), "caf\u{fffd}");
    }

    #[rstest]
    #[case::a_drive(r"C:\Users\me\repo", "C:/Users/me/repo")]
    #[case::a_verbatim_drive(r"\\?\C:\Users\me\repo", "C:/Users/me/repo")]
    #[case::a_verbatim_share(r"\\?\UNC\server\share\repo", "//server/share/repo")]
    #[case::a_relative_path(r"src\año.rs", "src/año.rs")]
    fn a_path_goes_to_git_with_slashes_and_no_verbatim_prefix(#[case] native: &str, #[case] printed: &str) {
        assert_eq!(&*to_bytes(Path::new(native)), printed.as_bytes());
        assert_eq!(text(Path::new(native)), printed);
    }

    #[rstest]
    #[case::a_verbatim_drive(r"\\?\C:\Users\me", r"C:\Users\me")]
    #[case::a_verbatim_share(r"\\?\UNC\server\share\x", r"\\server\share\x")]
    #[case::a_plain_drive(r"C:\Users\me", r"C:\Users\me")]
    #[case::a_device(r"\\.\pipe\cornercase", r"\\.\pipe\cornercase")]
    fn plain_drops_only_the_verbatim_prefix(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(plain(Path::new(path)).as_os_str(), expected);
    }

    #[rstest]
    #[case::slashes("C:/Users/me/repo", r"C:\Users\me\repo")]
    #[case::the_drive_s_case(r"c:\Users\me\repo", r"C:\Users\me\repo")]
    #[case::a_verbatim_prefix(r"\\?\C:\Users\me\repo", "C:/Users/me/repo")]
    fn paths_from_git_and_from_std_compare_equal(#[case] a: &str, #[case] b: &str) {
        assert!(same(Path::new(a), Path::new(b)));
    }

    #[test]
    fn only_the_drive_letter_s_case_is_ignored() {
        assert!(!same(Path::new(r"C:\Users\me\Repo"), Path::new(r"C:\Users\me\repo")));
    }

    #[test]
    fn a_relative_path_written_by_git_joins_onto_a_verbatim_folder() {
        let joined = join(Path::new(r"\\?\C:\repo\.git\worktrees\x"), "../..");
        assert_eq!(joined.as_os_str(), r"C:\repo\.git");
        assert_eq!(join(Path::new(r"\\?\C:\repo"), "C:/elsewhere/.git").as_os_str(), r"C:\elsewhere\.git");
    }

    #[test]
    fn a_path_list_is_split_on_semicolons() {
        assert_eq!(LIST_SEPARATOR, b';');
    }
}
