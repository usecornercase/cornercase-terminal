use std::path::PathBuf;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::plugins::registry::Source;
use crate::test_util::{FakeHttp, fixture};

const INDEX: &str = fixture!("plugins/index-v1.json");

fn index() -> Value {
    serde_json::from_str(INDEX).expect("fixture")
}

fn with(field: &str, value: Value) -> Vec<u8> {
    let mut index = index();
    index["plugins"][0][field] = value;
    serde_json::to_vec(&index).expect("json")
}

mod parsing {
    use super::*;

    #[test]
    fn the_shared_fixture_parses_with_every_field() {
        let index = parse(INDEX.as_bytes()).expect("valid");

        let hello = &index.plugins[0];
        assert_eq!(index.version, 1);
        assert_eq!((hello.id.as_str(), hello.repo.as_str()), ("example.hello", "example/cornercase-hello"));
        assert_eq!(hello.reference, "v0.1.0");
        assert_eq!(hello.permissions, ["status", "notify"]);
        assert_eq!(hello.sha256.len(), 1);
        assert_eq!(index.plugins[1].platforms, Vec::<String>::new());
    }

    #[test]
    fn the_published_index_parses() {
        let published = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/site/plugins/index.json"));

        assert!(parse(published.as_bytes()).is_ok());
    }

    #[test]
    fn unknown_fields_are_ignored_so_additions_do_not_break_older_cornercase() {
        let index = parse(&with("stars", json!(12))).expect("valid");

        assert_eq!(index.plugins.len(), 2);
    }

    #[rstest]
    #[case::newer(json!(2), "version 2")]
    #[case::missing(Value::Null, "no version")]
    #[case::text(json!("1"), "no version")]
    fn an_unknown_version_is_refused(#[case] version: Value, #[case] says: &str) {
        let mut index = index();
        index["version"] = version;

        let error = parse(&serde_json::to_vec(&index).expect("json")).expect_err("refused");

        assert!(error.contains(says), "{error}");
    }

    #[rstest]
    #[case::number_for_text("name", json!(3))]
    #[case::text_for_list("platforms", json!("macos"))]
    #[case::missing_commit("commit", Value::Null)]
    #[case::short_commit("commit", json!("0123abc"))]
    #[case::upper_case_commit("commit", json!("0123456789ABCDEF0123456789ABCDEF01234567"))]
    #[case::bad_id("id", json!("Hello World"))]
    #[case::url_for_repo("repo", json!("https://github.com/example/hello"))]
    #[case::three_parts("repo", json!("example/hello/more"))]
    #[case::owner_with_dash("repo", json!("-example/hello"))]
    #[case::dot_repo("repo", json!("example/.."))]
    #[case::option_repo("repo", json!("example/-u"))]
    #[case::option_ref("ref", json!("--upload-pack=x"))]
    #[case::escape_in_name("name", json!("Hello\u{1b}[31m"))]
    #[case::bidi_in_description("description", json!("safe \u{202e}txt.exe"))]
    #[case::empty_description("description", json!(" "))]
    #[case::long_description("description", json!("x".repeat(201)))]
    #[case::unknown_platform("platforms", json!(["windows"]))]
    #[case::unknown_target("sha256", json!({"riscv64": "a".repeat(64)}))]
    #[case::short_sum("sha256", json!({"aarch64-apple-darwin": "abc"}))]
    #[case::many_permissions("permissions", json!(vec!["p"; 17]))]
    fn a_bad_entry_refuses_the_whole_index(#[case] field: &str, #[case] value: Value) {
        assert!(parse(&with(field, value)).is_err());
    }

    #[test]
    fn the_same_id_twice_is_refused() {
        let mut index = index();
        index["plugins"][1]["id"] = json!("example.hello");

        let error = parse(&serde_json::to_vec(&index).expect("json")).expect_err("refused");

        assert!(error.contains("twice"), "{error}");
    }

    #[test]
    fn a_file_over_the_cap_is_refused_before_parsing() {
        let big = vec![b' '; usize::try_from(MAX_BYTES).expect("fits") + 1];

        assert!(parse(&big).expect_err("refused").contains("larger"));
    }

    #[test]
    fn too_many_plugins_are_refused() {
        let mut index = index();
        let entry = index["plugins"][1].clone();
        index["plugins"] = Value::Array(
            (0..=MAX_PLUGINS)
                .map(|i| {
                    let mut entry = entry.clone();
                    entry["id"] = json!(format!("p{i}"));
                    entry
                })
                .collect(),
        );

        assert!(parse(&serde_json::to_vec(&index).expect("json")).is_err());
    }

    #[test]
    fn clean_turns_control_and_bidi_characters_into_spaces() {
        assert_eq!(clean("a\u{1b}b\u{202e}c\nd"), "a b c d");
    }
}

mod marks {
    use super::*;

    fn entry() -> Entry {
        parse(INDEX.as_bytes()).expect("valid").plugins.remove(1)
    }

    #[test]
    fn a_plugin_of_the_same_id_is_installed() {
        let installed = [Installed {
            id: "example.minimal".into(),
            source: Source::Link { path: PathBuf::from("/code/minimal") },
            enabled: false,
        }];

        assert_eq!(entry().mark(&installed), Mark::Installed("linked from /code/minimal".into()));
        assert_eq!(entry().mark(&[]), Mark::Available);
    }

    #[test]
    fn another_platform_or_a_newer_cornercase_cannot_install() {
        let other = if manifest::platform() == "macos" { "linux" } else { "macos" };
        let elsewhere = Entry { platforms: vec![other.into()], ..entry() };
        let newer = Entry { min_cornercase: Some("999.0.0".into()), ..entry() };

        assert_eq!(elsewhere.mark(&[]), Mark::Unfit(format!("runs on {other} only")));
        assert_eq!(newer.mark(&[]), Mark::Unfit("needs cornercase 999.0.0 or later".into()));
    }
}

mod fetching {
    use super::*;

    #[test]
    fn fetch_reads_and_parses_the_index() {
        let server = FakeHttp::start(vec![("GET /plugins/index.json", 200, INDEX)]);

        let index = fetch(&format!("{}/plugins/index.json", server.url())).expect("fetched");

        assert_eq!(index.plugins.len(), 2);
    }

    #[rstest]
    #[case::status(404, b"{}".to_vec(), "404")]
    #[case::too_big(200, vec![b' '; 2 * 1024 * 1024], "more than 1024 KiB")]
    #[case::not_json(200, b"<html>".to_vec(), "the plugin catalogue")]
    fn fetch_says_why_it_failed(#[case] status: u16, #[case] body: Vec<u8>, #[case] says: &str) {
        let server = FakeHttp::start(vec![("GET /index.json", status, body)]);

        let error = fetch(&format!("{}/index.json", server.url())).expect_err("fails").to_string();

        assert!(error.contains(says), "{error}");
    }
}
