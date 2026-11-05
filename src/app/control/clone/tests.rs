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
#[case::unfit_files(Failure::Files("the `data` of files[0] is not base64".into()), Code::Malformed)]
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
    let url = crate::test_util::os_path(url);
    assert_eq!(checked_url(url.clone()), Ok(url));
}

mod the_files {
    use super::*;

    fn sized(bytes: u64) -> CloneFile {
        let data = STANDARD.encode(vec![b'x'; usize::try_from(bytes).expect("a size")]);
        CloneFile { path: PathBuf::from("a.bin"), data, mode: 0o600 }
    }

    #[test]
    fn up_to_the_limit_are_taken_though_their_base64_is_padded() {
        let file = sized(MAX_TOTAL_BYTES);
        assert!(file.data.ends_with('='), "padded");

        let files = decoded(&[file]).expect("taken");

        assert_eq!(files[0].bytes.len() as u64, MAX_TOTAL_BYTES);
    }

    #[test]
    fn over_the_limit_are_refused_before_their_base64_is_read() {
        let unread = CloneFile { data: "!".repeat(usize::try_from(MAX_TOTAL_BYTES).expect("a size") * 2), ..sized(0) };

        let Err(Failure::Files(why)) = decoded(&[sized(1), unread]) else { panic!("refused for its size") };

        assert!(why.contains("over the 8 MiB"), "{why}");
    }

    #[test]
    fn over_the_count_are_refused_even_when_empty() {
        let files = vec![sized(0); MAX_FILES + 1];

        let Err(Failure::Files(why)) = decoded(&files) else { panic!("refused for their count") };

        assert!(why.contains("over the 200 a clone takes"), "{why}");
        assert_eq!(decoded(&files[1..]).map(|taken| taken.len()).ok(), Some(MAX_FILES));
    }

    #[test]
    fn that_are_not_base64_are_named_by_their_place_and_never_their_path() {
        let file = CloneFile { path: PathBuf::from("secrets/.env"), data: "not base64!".into(), mode: 0o600 };

        let Err(Failure::Files(why)) = decoded(&[sized(3), file]) else { panic!("refused") };

        assert!(why.starts_with("the `data` of files[1] is not base64"), "{why}");
        assert!(!why.contains(".env"), "{why}");
    }
}
