use super::*;

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
    let local = Origin::parse("/srv/notes").expect("folder");
    let remote = Origin::parse("acme/notes").expect("github");
    assert_eq!((local.protocol(), remote.protocol()), ("file", "https"));
}
