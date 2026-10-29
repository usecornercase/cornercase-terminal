use rstest::rstest;

use super::*;

#[rstest]
#[case::no_name(Failure::NoName, Code::Malformed)]
#[case::named_git(Failure::GitFolder, Code::Malformed)]
#[case::not_empty(Failure::NotEmpty(PathBuf::from("/srv/shop")), Code::Conflict)]
#[case::no_folder(Failure::Folder("`/srv`: Permission denied".into()), Code::Failed)]
#[case::not_a_repository(Failure::NotRepository("Repository not found".into()), Code::NotFound)]
#[case::access_denied(Failure::AccessDenied("Permission denied (publickey)".into()), Code::Failed)]
#[case::other(Failure::Failed("Connection refused".into()), Code::Failed)]
fn each_failure_has_its_code(#[case] failure: Failure, #[case] code: Code) {
    let refused = refusal(&failure);

    assert_eq!((refused.code, refused.message), (code, failure.to_string()));
}

#[rstest]
#[case::relative("./shop")]
#[case::bare_name("shop")]
#[case::parent("../repos/shop")]
fn a_relative_path_to_a_repository_is_refused(#[case] url: &str) {
    let refused = checked_url(url.to_string()).expect_err("refused");

    assert_eq!(refused.code, Code::Malformed);
    assert!(!refused.message.contains(url), "{}", refused.message);
}

#[rstest]
#[case::absolute("/srv/git/shop")]
#[case::https("https://github.com/acme/shop")]
#[case::scp_like("git@github.com:acme/shop.git")]
fn other_urls_are_taken(#[case] url: &str) {
    assert_eq!(checked_url(url.to_string()), Ok(url.to_string()));
}
