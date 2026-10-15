use super::*;

#[rstest::rstest]
#[case::owner_and_repo("acme/notes", "acme/notes", "https://github.com/acme/notes.git", None)]
#[case::with_a_tag("acme/notes@v1.2", "acme/notes", "https://github.com/acme/notes.git", Some("v1.2"))]
#[case::with_git("acme/notes.git", "acme/notes", "https://github.com/acme/notes.git", None)]
#[case::a_github_address(
    "https://github.com/acme/notes.git@main",
    "acme/notes",
    "https://github.com/acme/notes.git",
    Some("main")
)]
#[case::another_host("https://git.example.com/a/b", "https://git.example.com/a/b", "https://git.example.com/a/b", None)]
#[case::a_folder("/srv/notes.git@feature/x", "/srv/notes.git", "/srv/notes.git", Some("feature/x"))]
fn these_are_installed_from(
    #[case] spec: &str,
    #[case] repo: &str,
    #[case] url: &str,
    #[case] reference: Option<&str>,
) {
    let origin = Origin::parse(spec).expect("parse");
    assert_eq!((origin.repo.as_str(), origin.url.as_str(), origin.reference.as_deref()), (repo, url, reference));
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
fn these_are_refused(#[case] spec: &str) {
    assert!(Origin::parse(spec).is_err(), "{spec}");
}

#[test]
fn a_folder_uses_the_file_protocol_and_the_rest_https() {
    let local = Origin::parse("/srv/notes").expect("folder");
    let remote = Origin::parse("acme/notes").expect("github");
    assert_eq!((local.protocol(), remote.protocol()), ("file", "https"));
}
