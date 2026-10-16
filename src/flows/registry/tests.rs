use rstest::rstest;

use super::*;
use crate::test_util::FakeHttp;

mod a_source {
    use super::*;

    #[rstest]
    #[case::stdin("-", false, Ok(Source::Stdin))]
    #[case::https("https://example.com/x.toml", false, Ok(Source::Url("https://example.com/x.toml".into())))]
    #[case::http("http://example.com/x.toml", false, Err("use an https:// address"))]
    #[case::repo("octocat/security-audit", false, Ok(Source::Repo("octocat/security-audit".into())))]
    #[case::file_beats_repo("octocat/flow", true, Ok(Source::File("octocat/flow".into())))]
    #[case::relative_path("./x.toml", false, Ok(Source::File("./x.toml".into())))]
    #[case::parent_path("../x", false, Ok(Source::File("../x".into())))]
    #[case::deep_path("a/b/c.toml", false, Ok(Source::File("a/b/c.toml".into())))]
    #[case::odd("Some Text", false, Ok(Source::File("Some Text".into())))]
    fn is_told_apart(#[case] arg: &str, #[case] exists: bool, #[case] expected: Result<Source, &str>) {
        assert_eq!(source(arg, |_| exists), expected.map_err(ToString::to_string));
    }

    #[rstest]
    #[case("octocat/hello-world", true)]
    #[case("a.b/c_d", true)]
    #[case("octocat", false)]
    #[case("a/b/c", false)]
    #[case("/b", false)]
    #[case("a/", false)]
    #[case("a b/c", false)]
    #[case("../c", false)]
    fn repos_are_owner_slash_name(#[case] text: &str, #[case] yes: bool) {
        assert_eq!(is_repo(text), yes);
    }

    #[test]
    fn typed_text_is_fetched_only_on_one_line() {
        assert_eq!(typed("  octocat/flow "), Some(Source::Repo("octocat/flow".into())));
        assert!(matches!(typed("https://example.com/x.toml"), Some(Source::Url(_))));
        assert_eq!(typed("version = 1\nname = \"x\""), None);
        assert_eq!(typed("http://example.com/x.toml"), None);
        assert_eq!(typed("Some Text"), None);
        assert_eq!(typed("-"), None);
    }

    #[test]
    fn a_repo_is_read_from_its_default_branch() {
        let urls = Source::Repo("octocat/flow".into()).urls();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].ends_with("/octocat/flow/HEAD/flow.toml"), "{urls:?}");
        assert!(urls[1].ends_with("/octocat/flow/HEAD/cornercase-flow.toml"), "{urls:?}");
        assert_eq!(Source::Repo("octocat/flow".into()).page(), "https://github.com/octocat/flow");
        assert_eq!(Source::Repo("octocat/flow".into()).address(), urls[0]);
    }
}

mod fetching {
    use super::*;

    fn text(server: &FakeHttp, path: &str) -> Result<String, String> {
        fetch(&Source::Url(format!("{}{path}", server.url())))
    }

    #[test]
    fn reads_the_text() {
        let server = FakeHttp::start(vec![("GET /flows/x.toml", 200, "name = \"x\"")]);
        assert_eq!(text(&server, "/flows/x.toml"), Ok("name = \"x\"".to_string()));
    }

    #[test]
    fn a_missing_file_says_so() {
        let server = FakeHttp::start(Vec::<(&str, u16, &str)>::new());
        let err = text(&server, "/flows/x.toml").expect_err("404");
        assert!(err.starts_with("nothing at "), "{err}");
    }

    #[test]
    fn a_failure_names_the_url_and_the_way_out() {
        let server = FakeHttp::start(vec![("GET /x.toml", 500, "")]);
        let err = text(&server, "/x.toml").expect_err("500");
        assert!(err.contains("answered 500") && err.contains("import it from its path"), "{err}");
    }

    #[test]
    fn a_long_answer_is_refused() {
        let body = "a".repeat(share::MAX_BYTES + 10);
        let server = FakeHttp::start(vec![("GET /big.toml", 200, body)]);
        let err = text(&server, "/big.toml").expect_err("too long");
        assert_eq!(err, format!("{}/big.toml is too long for a flow (at most 256 KiB)", server.url()));
    }

    #[test]
    fn an_answer_one_byte_over_is_refused_the_same_way() {
        let server = FakeHttp::start(vec![("GET /edge.toml", 200, "a".repeat(share::MAX_BYTES + 1))]);
        let err = text(&server, "/edge.toml").expect_err("too long");
        assert_eq!(err, format!("{}/edge.toml is too long for a flow (at most 256 KiB)", server.url()));
    }

    #[test]
    fn a_redirect_is_followed() {
        let (server, elsewhere) = crate::test_util::redirected("GET /moved.toml");
        assert_eq!(text(&server, "/moved.toml"), Ok("{}".to_string()));
        drop(elsewhere);
    }
}
