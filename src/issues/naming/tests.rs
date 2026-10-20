use rstest::rstest;

use super::*;
use crate::issues::issue;

mod rendering {
    use super::*;

    #[rstest]
    #[case::github(Source::Github, "feat/{key}-{slug}", "feat/482-dark-mode")]
    #[case::linear(Source::Linear, "feat/{key}-{slug}", "feat/ENG-482-dark-mode")]
    #[case::number(Source::Linear, "{number}-{slug}", "482-dark-mode")]
    #[case::shortcut_number(Source::Shortcut, "{source}/{number}", "shortcut/482")]
    #[case::user(Source::Jira, "{user}/{key}", "ada-lovelace/PROJ-482")]
    #[case::spaces(Source::Github, "fix {key} {slug}", "fix-482-dark-mode")]
    #[case::odd_characters(Source::Github, "feat~/{key}:{slug}", "feat/482-dark-mode")]
    fn fills_the_placeholders_and_cleans_the_result(
        #[case] source: Source,
        #[case] template: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(branch(&issue(source, 482, "Dark mode!"), template, "Ada Lovelace"), expected);
    }

    #[test]
    fn an_empty_template_keeps_each_trackers_default() {
        for source in Source::ALL {
            let issue = issue(source, 7, "Fix login");
            assert_eq!(branch(&issue, "  ", "ada"), crate::issues::branch(&issue));
        }
    }

    #[test]
    fn an_unknown_user_drops_its_part() {
        assert_eq!(branch(&issue(Source::Linear, 42, "x"), "{user}/{key}", ""), "ENG-42");
    }

    #[test]
    fn a_broken_template_or_an_empty_result_falls_back_to_the_default() {
        let issue = issue(Source::Github, 7, "Fix login");
        assert_eq!(branch(&issue, "{oops}", ""), "issue-7-fix-login");
        assert_eq!(branch(&issue, "///", ""), "issue-7-fix-login");
    }

    #[test]
    fn an_asana_task_without_an_id_field_uses_its_gid_as_number() {
        let task =
            Issue { key: "1200000000000001".into(), number: 1_200_000_000_000_001, ..issue(Source::Asana, 0, "x") };
        assert_eq!(branch(&task, "t/{number}", ""), "t/1200000000000001");
    }

    #[rstest]
    #[case::dots("a..b", "a.b")]
    #[case::lock("feat/x.lock", "feat/x")]
    #[case::edges("-/.hidden/./x-", "hidden/x")]
    #[case::repeats("a--b//c", "a-b/c")]
    fn cleaning_makes_a_name_git_takes(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(clean(text), expected);
        assert!(check_branch(expected).is_ok());
    }
}

mod checking {
    use super::*;

    #[rstest]
    #[case::known("feat/{key}-{slug}", Ok("feat/{key}-{slug}".into()))]
    #[case::empty("  ", Ok(String::new()))]
    #[case::trimmed(" {number} ", Ok("{number}".into()))]
    #[case::unknown("{id}-{slug}", Err("unknown placeholder {id}: use {key}, {number}, {slug}, {source} or {user}".into()))]
    #[case::open("{key", Err("a { has no }".into()))]
    #[case::close("key}", Err("a } has no {".into()))]
    #[case::no_key("feat/{slug}", Err("the template needs {key} or {number}, so each issue gets a branch of its own".into()))]
    fn refuses_what_would_not_name_one_branch_per_issue(
        #[case] template: &str,
        #[case] expected: Result<String, String>,
    ) {
        assert_eq!(check(template), expected);
    }

    #[rstest]
    #[case::fine("feat/ENG-42")]
    #[case::slashes("ada/fix/42-x")]
    fn takes_a_valid_branch(#[case] name: &str) {
        assert_eq!(check_branch(&format!(" {name} ")), Ok(name.to_string()));
    }

    #[rstest]
    #[case::empty("")]
    #[case::space("my branch")]
    #[case::colon("a:b")]
    #[case::dash("-x")]
    #[case::dots("a..b")]
    #[case::at("a@{b")]
    #[case::slash("a/")]
    #[case::dot("a.")]
    #[case::hidden("a/.b")]
    #[case::lock("a.lock")]
    fn refuses_what_git_would(#[case] name: &str) {
        assert!(check_branch(name).is_err());
    }
}

mod notes {
    use super::*;

    #[test]
    fn shows_an_example() {
        let example = "such as 482-ada-empty-address-crashes-the-returns-page";
        assert_eq!(note("{number}-{user}-{slug}"), (example.into(), false));
    }

    #[test]
    fn warns_when_gitlab_loses_its_number_prefix() {
        let (text, warns) = note("feat/{key}-{slug}");
        assert!(warns);
        assert!(text.ends_with("GitLab closes issues only from {number}-… branches"), "{text}");
        assert!(!note("{number}-{slug}").1);
    }

    #[test]
    fn says_what_empty_means() {
        assert_eq!(note("").0, "empty: issue-482-…, 482-… on GitLab, sc-482-…, ENG-482-…");
    }
}
