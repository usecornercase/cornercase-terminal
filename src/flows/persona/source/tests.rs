use std::io::Write as _;

use flate2::Compression;
use flate2::write::GzEncoder;
use rstest::rstest;

use super::*;
use crate::test_util::{FakeHttp, TempDir};

const REVIEWER: &str = "---\nname: Code Reviewer\n---\nReview it.\n";
const TESTER: &str = "---\nname: Tester\n---\nTest it.\n";
const README: &str = "# Agents\n";
const DIVISIONS_JSON: &str = r#"{"divisions": {"engineering": {"label": "Engineering"}, "testing": {}}}"#;

fn hosts(server: &FakeHttp) -> Hosts {
    Hosts { raw: server.url(), archive: server.url() }
}

fn archive_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (path, text) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, format!("agency-agents-HEAD/{path}"), text.as_bytes()).expect("append");
    }
    builder.into_inner().expect("tar").finish().expect("gzip")
}

fn ids(gathered: &Gathered) -> Vec<(&str, Option<&str>)> {
    gathered.found.iter().map(|found| (found.file.as_str(), found.source.as_deref())).collect()
}

mod a_source {
    use super::*;

    #[rstest]
    #[case::stdin("-", None, Ok(Source::Stdin))]
    #[case::https("https://example.com/r.md", None, Ok(Source::Url("https://example.com/r.md".into())))]
    #[case::http("http://example.com/r.md", None, Err("use an https:// address"))]
    #[case::repo("octo/agents", None, Ok(Source::Repo { repo: "octo/agents".into(), path: String::new() }))]
    #[case::repo_folder(
        "octo/agents/engineering/",
        None,
        Ok(Source::Repo { repo: "octo/agents".into(), path: "engineering".into() })
    )]
    #[case::repo_file(
        "octo/agents/engineering/reviewer.md",
        None,
        Ok(Source::Repo { repo: "octo/agents".into(), path: "engineering/reviewer.md".into() })
    )]
    #[case::folder_beats_repo("octo/agents", Some(true), Ok(Source::Folder("octo/agents".into())))]
    #[case::file_beats_repo("octo/agents.md", Some(false), Ok(Source::File("octo/agents.md".into())))]
    #[case::parent("../agents/r.md", None, Ok(Source::File("../agents/r.md".into())))]
    #[case::odd("Some Text", None, Ok(Source::File("Some Text".into())))]
    fn is_told_apart(#[case] arg: &str, #[case] on_disk: Option<bool>, #[case] expected: Result<Source, &str>) {
        assert_eq!(source(arg, |_| on_disk), expected.map_err(ToString::to_string));
    }

    #[test]
    fn a_repository_links_to_its_page() {
        let at = |path: &str| Source::Repo { repo: "octo/agents".into(), path: path.into() };
        assert_eq!(at("").page(), "https://github.com/octo/agents");
        assert_eq!(at("engineering").page(), "https://github.com/octo/agents/tree/HEAD/engineering");
        assert_eq!(at("engineering").label(), "octo/agents/engineering");
        assert!(at("").is_web() && !Source::File("x.md".into()).is_web());
    }
}

mod on_disk {
    use super::*;

    #[test]
    fn a_file_is_one_persona_with_its_path_as_source() {
        let dir = TempDir::new();
        let file = dir.path().join("reviewer.md");
        std::fs::write(&file, REVIEWER).expect("write");

        let gathered = gather(&Source::File(file.clone()), &Hosts::from_env()).expect("gathered");

        assert_eq!(ids(&gathered), [("reviewer.md", Some(file.display().to_string().as_str()))]);
    }

    #[test]
    fn a_file_that_is_not_a_persona_is_refused() {
        let dir = TempDir::new();
        let file = dir.path().join("README.md");
        std::fs::write(&file, README).expect("write");

        let error = gather(&Source::File(file), &Hosts::from_env()).expect_err("not a persona");

        assert!(error.contains("is not a persona"), "{error}");
    }

    #[test]
    fn a_folder_gives_every_persona_in_it_and_skips_the_rest() {
        let dir = TempDir::new();
        for (path, text) in [
            ("engineering/reviewer.md", REVIEWER),
            ("testing/deep/tester.md", TESTER),
            ("README.md", README),
            (".hidden/secret.md", TESTER),
            ("notes.txt", REVIEWER),
        ] {
            let file = dir.path().join(path);
            std::fs::create_dir_all(file.parent().expect("a parent")).expect("folders");
            std::fs::write(file, text).expect("write");
        }

        let gathered = gather(&Source::Folder(dir.path().to_path_buf()), &Hosts::from_env()).expect("gathered");

        let files: Vec<&str> = gathered.found.iter().map(|found| found.file.as_str()).collect();
        assert_eq!(files, ["reviewer.md", "tester.md"]);
        assert_eq!(gathered.skipped, 1);
    }

    #[test]
    fn a_folder_without_personas_says_so() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("README.md"), README).expect("write");

        let error = gather(&Source::Folder(dir.path().to_path_buf()), &Hosts::from_env()).expect_err("none");

        assert!(error.starts_with("no personas in "), "{error}");
    }
}

mod from_the_web {
    use super::*;

    #[test]
    fn an_address_is_one_persona() {
        let server = FakeHttp::start(vec![("GET /p/reviewer.md", 200, REVIEWER)]);
        let url = format!("{}/p/reviewer.md?next=/download#a/b", server.url());

        let gathered = gather(&Source::Url(url.clone()), &hosts(&server)).expect("gathered");

        assert_eq!(ids(&gathered), [("reviewer.md", Some(url.as_str()))]);
    }

    #[test]
    fn a_file_in_a_repository_is_read_from_its_default_branch() {
        let server = FakeHttp::start(vec![("GET /octo/agents/HEAD/engineering/reviewer.md", 200, REVIEWER)]);
        let source = Source::Repo { repo: "octo/agents".into(), path: "engineering/reviewer.md".into() };

        let gathered = gather(&source, &hosts(&server)).expect("gathered");

        assert_eq!(ids(&gathered), [("reviewer.md", Some("octo/agents/engineering/reviewer.md"))]);
    }

    #[test]
    fn a_missing_file_names_its_address() {
        let server = FakeHttp::start(Vec::<(&str, u16, &str)>::new());
        let source = Source::Repo { repo: "octo/agents".into(), path: "nope.md".into() };

        let error = gather(&source, &hosts(&server)).expect_err("404");

        assert_eq!(error, format!("nothing at {}/octo/agents/HEAD/nope.md", server.url()));
    }

    #[test]
    fn a_long_answer_is_refused() {
        let server = FakeHttp::start(vec![("GET /big.md", 200, "a".repeat(MAX_BYTES + 1))]);
        let url = format!("{}/big.md", server.url());

        let error = gather(&Source::Url(url.clone()), &hosts(&server)).expect_err("too long");

        assert_eq!(error, format!("{url} is too long for a persona (at most 256 KiB)"));
    }

    #[test]
    fn a_repository_is_one_download_read_in_the_folders_its_divisions_name() {
        let archive = archive_of(&[
            ("divisions.json", DIVISIONS_JSON),
            ("engineering/reviewer.md", REVIEWER),
            ("testing/tester.md", TESTER),
            ("integrations/copy/reviewer-copy.md", REVIEWER),
            ("README.md", README),
        ]);
        let server = FakeHttp::start(vec![("GET /octo/agents/tar.gz/HEAD", 200, archive)]);
        let source = Source::Repo { repo: "octo/agents".into(), path: String::new() };

        let gathered = gather(&source, &hosts(&server)).expect("gathered");

        assert_eq!(
            ids(&gathered),
            [
                ("reviewer.md", Some("octo/agents/engineering/reviewer.md")),
                ("tester.md", Some("octo/agents/testing/tester.md"))
            ]
        );
        assert_eq!(gathered.skipped, 2);
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn a_folder_of_a_repository_is_read_alone() {
        let archive = archive_of(&[("engineering/reviewer.md", REVIEWER), ("testing/tester.md", TESTER)]);
        let server = FakeHttp::start(vec![("GET /octo/agents/tar.gz/HEAD", 200, archive)]);
        let source = Source::Repo { repo: "octo/agents".into(), path: "testing".into() };

        let gathered = gather(&source, &hosts(&server)).expect("gathered");

        assert_eq!(ids(&gathered), [("tester.md", Some("octo/agents/testing/tester.md"))]);
    }

    #[test]
    fn a_missing_repository_says_so() {
        let server = FakeHttp::start(Vec::<(&str, u16, &str)>::new());
        let source = Source::Repo { repo: "octo/gone".into(), path: String::new() };

        let error = gather(&source, &hosts(&server)).expect_err("404");

        assert_eq!(error, "no repository octo/gone on GitHub, or it is private");
    }

    #[test]
    fn a_download_that_is_not_an_archive_is_refused() {
        let mut broken = GzEncoder::new(Vec::new(), Compression::fast());
        broken.write_all(b"not a tar").expect("gzip");
        let server = FakeHttp::start(vec![("GET /octo/agents/tar.gz/HEAD", 200, broken.finish().expect("gzip"))]);
        let source = Source::Repo { repo: "octo/agents".into(), path: String::new() };

        let error = gather(&source, &hosts(&server)).expect_err("broken");

        assert!(
            error.starts_with("cannot read the download of octo/agents") || error.starts_with("no personas"),
            "{error}"
        );
    }
}
