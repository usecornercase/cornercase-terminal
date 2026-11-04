use super::*;
use crate::test_util::os_path;

type Parsed<'a> = (&'a str, &'a str, Option<&'a str>, Option<&'a str>);

const HUB: &str = "https://github.com/acme/notes.git";

#[rstest::rstest]
#[case::owner_and_repo("acme/notes", ("acme/notes", HUB, None, None))]
#[case::with_a_tag("acme/notes@v1.2", ("acme/notes", HUB, Some("v1.2"), None))]
#[case::with_git("acme/notes.git", ("acme/notes", HUB, None, None))]
#[case::a_github_address("https://github.com/acme/notes.git@main", ("acme/notes", HUB, Some("main"), None))]
#[case::another_host("https://git.example.com/a/b", ("https://git.example.com/a/b", "https://git.example.com/a/b", None, None))]
#[case::a_folder("/srv/notes.git@feature/x", ("/srv/notes.git", "/srv/notes.git", Some("feature/x"), None))]
#[case::a_subfolder("acme/notes/plugins/one@v1", ("acme/notes", HUB, Some("v1"), Some("plugins/one")))]
#[case::a_subfolder_of_a_github_address("https://github.com/acme/notes/plugins/one", ("acme/notes", HUB, None, Some("plugins/one")))]
#[case::a_marked_subfolder_of_a_github_address("https://github.com/acme/notes.git//one", ("acme/notes", HUB, None, Some("one")))]
#[case::a_marked_subfolder_elsewhere("https://git.example.com/a/b.git//plugins/one@v2", ("https://git.example.com/a/b.git", "https://git.example.com/a/b.git", Some("v2"), Some("plugins/one")))]
#[case::a_marked_subfolder_of_a_folder("/srv/notes.git//one", ("/srv/notes.git", "/srv/notes.git", None, Some("one")))]
fn these_are_installed_from(#[case] spec: &str, #[case] expected: Parsed<'_>) {
    let origin = Origin::parse(spec).expect("parse");
    let parsed = (origin.repo.as_str(), origin.url.as_str(), origin.reference.as_deref(), origin.subdir.as_deref());
    assert_eq!(parsed, expected);
}

#[test]
fn a_subfolder_comes_back_from_its_spec() {
    let origin = Origin::parse("https://git.example.com/a/b.git//plugins/one@v2").expect("parse");
    assert_eq!(Origin::parse(&origin.spec()).expect("again"), origin);
}

#[rstest::rstest]
#[case::a_ref_like_an_option("acme/notes@--upload-pack=x")]
#[case::a_ref_going_up("acme/notes@a/../b")]
#[case::an_owner_like_an_option("-acme/notes")]
#[case::a_repo_like_an_option("acme/-notes")]
#[case::the_file_transport("file:///srv/notes")]
#[case::the_ext_transport("ext::sh -c touch% /tmp/x")]
#[case::the_ext_transport_without_spaces("ext::sh")]
#[case::ssh("ssh://git@github.com/acme/notes")]
#[case::scp_like("git@github.com:acme/notes")]
#[case::plain_http("http://github.com/acme/notes")]
#[case::credentials("https://user:token@github.com/acme/notes")]
#[case::no_repo("acme")]
#[case::spaces("acme/notes x")]
#[case::a_subfolder_going_up("acme/notes/plugins/../x")]
#[case::a_subfolder_with_a_dot("acme/notes/./x")]
#[case::an_empty_subfolder_part("acme/notes//x/")]
#[case::a_subfolder_like_an_option("acme/notes/-x")]
#[case::a_marked_subfolder_going_up("https://git.example.com/a//..")]
#[case::an_empty_marked_subfolder("/srv/notes.git//")]
fn these_are_refused(#[case] spec: &str) {
    assert!(Origin::parse(spec).is_err(), "{spec}");
}

#[test]
fn a_folder_uses_the_file_protocol_and_the_rest_https() {
    let local = Origin::parse(&os_path("/srv/notes")).expect("folder");
    let remote = Origin::parse("acme/notes").expect("github");
    assert_eq!((local.protocol(), remote.protocol()), ("file", "https"));
}

#[test]
fn git_gets_a_folder_or_an_address_as_written() {
    for spec in [os_path("/srv/notes.git"), "https://git.example.com/acme/notes.git".into()] {
        let origin = Origin::parse(&spec).expect("a source");
        assert_eq!(origin.address(), OsStr::new(&spec));
    }
}

#[test]
fn a_folder_on_this_computer_and_an_address_are_taken() {
    let dir = crate::test_util::TempDir::new();
    let folder = Origin::parse(&dir.path().display().to_string()).expect("a folder");
    let remote = Origin::parse("https://git.example.com/acme/notes.git").expect("an address");
    assert_eq!((folder.on_this_computer(), remote.on_this_computer()), (Ok(()), Ok(())));
}

#[cfg(windows)]
mod on_windows {
    use super::*;

    #[rstest::rstest]
    #[case::backslashes(r"C:\srv\notes.git", r"C:\srv\notes.git", None, None)]
    #[case::slashes("D:/srv/notes.git@v1", "D:/srv/notes.git", Some("v1"), None)]
    #[case::a_marked_subfolder(r"C:\srv\tools.git//notes@main", r"C:\srv\tools.git", Some("main"), Some("notes"))]
    #[case::verbatim(r"\\?\C:\srv\notes.git", r"\\?\C:\srv\notes.git", None, None)]
    fn a_folder_with_a_drive_is_installed_from_with_the_file_protocol(
        #[case] spec: &str,
        #[case] url: &str,
        #[case] reference: Option<&str>,
        #[case] subdir: Option<&str>,
    ) {
        let origin = Origin::parse(spec).expect("a folder");
        let parsed = (origin.url.as_str(), origin.reference.as_deref(), origin.subdir.as_deref(), origin.protocol());
        assert_eq!(parsed, (url, reference, subdir, "file"));
    }

    #[rstest::rstest]
    #[case::a_share(r"\\host\share\notes.git")]
    #[case::a_share_with_slashes("//host/share/notes.git")]
    #[case::a_verbatim_share(r"\\?\UNC\host\share\notes.git")]
    #[case::a_device(r"\\.\pipe\notes")]
    fn a_folder_on_the_network_is_refused(#[case] spec: &str) {
        let error = Origin::parse(spec).expect_err("refused");
        assert!(error.contains("on the network"), "{error}");
    }

    #[rstest::rstest]
    #[case::verbatim(r"\\?\C:\srv\notes.git", r"C:\srv\notes.git")]
    #[case::verbatim_with_a_subfolder(r"\\?\D:\srv\tools.git//notes@main", r"D:\srv\tools.git")]
    #[case::plain(r"C:\srv\notes.git", r"C:\srv\notes.git")]
    #[case::with_slashes("C:/srv/notes.git", "C:/srv/notes.git")]
    fn git_gets_a_folder_without_its_verbatim_prefix(#[case] spec: &str, #[case] address: &str) {
        assert_eq!(Origin::parse(spec).expect("a folder").address(), OsStr::new(address));
    }

    #[test]
    fn a_folder_that_leads_to_the_network_through_a_link_is_refused() {
        let dir = crate::test_util::TempDir::new();
        let link = dir.path().join("shared");
        crate::os::fs::make_junction(&link, Path::new(r"UNC\cornercase.invalid\share")).expect("a junction");
        let origin = Origin::parse(&link.join("tools.git").display().to_string()).expect("a folder");

        let error = origin.on_this_computer().expect_err("refused");

        assert!(error.contains("leads through a link") && error.contains("cornercase.invalid"), "{error}");
    }
}
