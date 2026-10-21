use rstest::rstest;
use serde_json::{Value, json};

use super::config::{MARKER, Setup, file, ok as configured, set, setup, shortcut};
use super::*;
use crate::control::config::{Described, NoteKind, SetupExported, SetupPreview};
use crate::settings::share::{self, Repository, Section, Shared};
use crate::test_util::{git, git_repo};
use crate::text;

const FIXED: [&str; 4] =
    ["tokens, bots, paired chats and browsers", "jira_email", "accounts and profiles", "remote access and keep awake"];

fn export(s: &mut Setup) -> SetupExported {
    serde_json::from_value(ok(&mut s.app, "config-export", &json!({}))).expect("an export")
}

fn read(exported: &SetupExported) -> Shared {
    share::parse(&exported.text).expect("a setup")
}

fn repository(s: &mut Setup, remotes: &[(&str, &str)], setup: &str) -> TempDir {
    let repo = git_repo(&[("a", "1")]);
    for (name, url) in remotes {
        git(repo.path(), &["remote", "add", name, url]);
    }
    let path = repo.path().display().to_string();
    configured(s, &json!({"action": "project", "entry": {"path": path, "setup": setup}}));
    repo
}

fn named(s: &Setup, folder: &Path, why: &str) -> String {
    format!("{} {why}, so its setup is not shared", text::display_path(folder, s.app.home.as_deref()))
}

mod exporting {
    use super::*;

    #[test]
    fn writes_the_settings_of_every_section() {
        let mut s = setup();
        set(&mut s, &json!({"sidebar": "tree"}));

        let exported = export(&mut s);

        assert_eq!(read(&exported).settings["sidebar"], json!("tree"));
        assert_eq!(exported.settings, share::settings_of(&s.app.config).len());
        assert_eq!(exported.left_out, FIXED);
    }

    #[test]
    fn keys_a_repository_by_its_remotes() {
        let mut s = setup();
        let remotes = [("alpha", "https://github.com/up/web-shop"), ("origin", "git@github.com:acme/web-shop.git")];
        let _repo = repository(&mut s, &remotes, "npm ci");

        let exported = export(&mut s);

        let wanted = Repository {
            remotes: vec!["github.com/acme/web-shop".into(), "github.com/up/web-shop".into()],
            setup: "npm ci".into(),
            teardown: String::new(),
        };
        assert_eq!((read(&exported).repositories, exported.repositories), (vec![wanted], 1));
    }

    #[test]
    fn names_a_project_without_a_remote_in_what_was_left_out() {
        let mut s = setup();
        let repo = repository(&mut s, &[], "make");

        let exported = export(&mut s);

        assert_eq!(exported.left_out[FIXED.len()..], [named(&s, repo.path(), "has no git remote")]);
        assert_eq!((read(&exported).repositories, exported.repositories), (Vec::new(), 0));
    }

    #[test]
    fn names_a_folder_that_is_no_longer_a_repository() {
        let mut s = setup();
        let repo = repository(&mut s, &[("origin", "git@github.com:acme/web-shop.git")], "make");
        std::fs::remove_dir_all(repo.path().join(".git")).expect("remove .git");

        let exported = export(&mut s);

        assert_eq!(exported.left_out[FIXED.len()..], [named(&s, repo.path(), "is not a repository any more")]);
    }

    #[test]
    fn shares_only_the_first_of_two_projects_with_a_remote_in_common() {
        let mut s = setup();
        let url = "git@github.com:acme/web-shop.git";
        let first = repository(&mut s, &[("origin", url)], "npm ci");
        let second = repository(&mut s, &[("origin", url)], "yarn");
        let (first, second) = if first.path() < second.path() { (first, second) } else { (second, first) };

        let exported = export(&mut s);

        let home = s.app.home.as_deref();
        let (second, first) = (text::display_path(second.path(), home), text::display_path(first.path(), home));
        let shares = format!("{second} shares a remote with {first}, so only the first is shared");
        assert_eq!(exported.left_out[FIXED.len()..], [shares]);
        assert_eq!(exported.repositories, 1);
    }

    #[test]
    fn never_holds_a_secret() {
        let mut s = setup();
        let _server = shortcut(&mut s, 200);
        configured(&mut s, &json!({"action": "secret", "entry": {"key": "shortcut", "value": MARKER}}));
        s.app
            .change_config(|config| {
                config.jira_email = "me@acme.com".into();
                config.profiles.insert("work".into(), [("claude".into(), "work".into())].into());
                config.remote = true;
                config.keep_awake = true;
            })
            .expect("change the config");

        let exported = export(&mut s);

        for secret in [MARKER, "me@acme.com", "profiles", "keep_awake", "remote_port"] {
            assert!(!exported.text.contains(secret), "{secret} in {}", exported.text);
        }
        assert!(!exported.text.lines().any(|line| line.starts_with("remote =")), "{}", exported.text);
        assert_eq!(read(&exported).connect, ["shortcut"]);
    }

    #[test]
    fn warns_about_a_secret_in_a_setup() {
        let mut s = setup();
        let _repo = repository(&mut s, &[("origin", "git@github.com:acme/web-shop.git")], "export NPM_TOKEN=abc");

        let exported = export(&mut s);

        assert_eq!(
            exported.warnings,
            ["setup of github.com/acme/web-shop sets NPM_TOKEN: check no secret goes with it"]
        );
    }
}

const SHOP_URL: &str = "git@github.com:acme/web-shop.git";

fn import_args(text: &str, sections: Option<&[&str]>, apply: bool) -> Value {
    json!({"text": text, "sections": sections, "apply": apply})
}

fn imported(s: &mut Setup, text: &str, sections: Option<&[&str]>) -> Described {
    let answer = ok(&mut s.app, "config-import", &import_args(text, sections, true));
    serde_json::from_value(answer).expect("a description")
}

fn previewed(s: &mut Setup, text: &str) -> SetupPreview {
    serde_json::from_value(ok(&mut s.app, "config-import", &import_args(text, None, false))).expect("a preview")
}

fn bytes(s: &Setup) -> Option<Vec<u8>> {
    std::fs::read(s.config.path().join("config.json")).ok()
}

fn open_clone(s: &mut Setup, remotes: &[(&str, &str)]) -> TempDir {
    let repo = git_repo(&[("a", "1")]);
    for (name, url) in remotes {
        git(repo.path(), &["remote", "add", name, url]);
    }
    s.app.open_project(repo.path().to_path_buf(), AREA).expect("open the repository");
    repo
}

fn shop_setup(setup: &str) -> String {
    format!(
        "version = 1\n[[repositories]]\nremotes = [\"github.com/acme/web-shop\"]\nsetup = \"{setup}\"\nteardown = \"\"\n"
    )
}

fn changed_everywhere(s: &mut Setup) {
    s.app
        .change_config(|config| {
            config.sidebar = "tree".into();
            config.counts = false;
            config.sound_subagents = true;
            config.prompt = "Work on {url}\nand open a pull request".into();
            config.agent_args.insert("claude".into(), vec!["--model".into(), "opus".into()]);
            config.agent_commands.insert("mine".into(), "/bin/sh".into());
            config.agent = "mine".into();
            config.submit = true;
            config.issue_branch = "{source}-{number}-{slug}".into();
            config.issue_tabs = vec!["jira".into(), "all".into()];
            config.jira_site = "acme.atlassian.net".into();
            config.worktrees_dir = "~/elsewhere".into();
            config.fetch_minutes = 9;
            config.gh = "/usr/local/bin/gh".into();
            config.jira_email = "me@acme.com".into();
        })
        .expect("change the config");
}

mod importing {
    use super::*;

    #[test]
    fn a_round_trip_into_a_fresh_config_gives_the_same_settings() {
        let mut from = setup();
        changed_everywhere(&mut from);
        let text = export(&mut from).text;
        let mut to = setup();
        to.app.change_config(|config| config.keep_awake = true).expect("change the config");

        let described = imported(&mut to, &text, None);

        let (wanted, got) = (file(&from), file(&to));
        for key in Section::ALL.iter().flat_map(|section| section.keys()) {
            assert_eq!(got[*key], wanted[*key], "{key}");
        }
        assert_eq!((got["jira_email"].clone(), got["keep_awake"].clone()), (json!(""), json!(true)));
        assert!(described.notices[0].starts_with("imported "), "{:?}", described.notices);
    }

    #[test]
    fn a_preview_changes_nothing() {
        let mut s = setup();
        set(&mut s, &json!({"counts": false}));
        let before = bytes(&s);

        let preview = previewed(&mut s, "version = 1\n[interface]\nsidebar = \"tree\"\ncounts = true\n");

        let interface = preview.sections.iter().find(|section| section.id == "interface").expect("interface");
        assert_eq!(interface.changes.len(), 2);
        assert_eq!(bytes(&s), before);
        assert!(!s.app.config.counts);
    }

    #[test]
    fn only_ticked_sections_are_applied() {
        let mut s = setup();
        let text = "version = 1\n[agents]\nprompt = \"Fix {url}\"\n[interface]\nsidebar = \"tree\"\n";

        let described = imported(&mut s, text, Some(&["interface"]));

        assert_eq!((s.app.config.sidebar.as_str(), s.app.config.prompt.as_str()), ("tree", "{url}"));
        assert_eq!(described.notices[0], "imported 1 setting and 0 repositories");
    }

    #[test]
    fn an_unticked_change_alone_writes_nothing() {
        let mut s = setup();
        set(&mut s, &json!({"counts": false}));
        let before = bytes(&s);

        let described = imported(&mut s, "version = 1\n[agents]\nprompt = \"Fix {url}\"\n", Some(&["interface"]));

        assert!(described.notices[0].starts_with("nothing to import"), "{:?}", described.notices);
        assert_eq!(bytes(&s), before);
    }

    #[test]
    fn keys_the_file_does_not_mention_stay() {
        let mut s = setup();
        set(&mut s, &json!({"counts": false}));

        imported(&mut s, "version = 1\n[interface]\nsidebar = \"tree\"\n", None);

        assert_eq!((s.app.config.sidebar.as_str(), s.app.config.counts), ("tree", false));
    }

    #[test]
    fn an_agent_defined_by_the_file_can_be_the_default() {
        let mut s = setup();
        let text = "version = 1\n[agents]\nagent = \"mine\"\nagent_commands = { mine = \"/bin/sh\" }\n";

        imported(&mut s, text, None);

        assert_eq!(s.app.config.agent, "mine");
        assert_eq!(file(&s)["agent_commands"], json!({"mine": "/bin/sh"}));
    }

    #[test]
    fn sets_the_setup_of_every_clone_of_a_remote() {
        let mut s = setup();
        let (first, second) =
            (open_clone(&mut s, &[("origin", SHOP_URL)]), open_clone(&mut s, &[("origin", SHOP_URL)]));
        let home = s.app.home.clone();
        let names = [first.path(), second.path()].map(|path| text::display_path(path, home.as_deref()));

        let preview = previewed(&mut s, &shop_setup("npm ci"));
        let described = imported(&mut s, &shop_setup("npm ci"), None);

        let repositories = preview.sections.iter().find(|section| section.id == "repositories").expect("repositories");
        let labels: Vec<&str> = repositories.changes.iter().map(|change| change.label.as_str()).collect();
        assert_eq!(labels, names);
        assert_eq!(described.notices[0], "imported 0 settings and 2 repositories");
        for repo in [&first, &second] {
            assert_eq!(s.app.config.project(repo.path(), home.as_deref()).setup, "npm ci");
        }
    }

    #[test]
    fn an_unknown_key_is_skipped_and_the_rest_imported() {
        let mut s = setup();
        let text = "version = 1\n[interface]\ncolours = \"plum\"\nsidebar = \"tree\"\n";

        let preview = previewed(&mut s, text);
        let described = imported(&mut s, text, None);

        let unknown = "this cornercase does not know interface.colours; update it to import that setting";
        assert!(preview.notes.iter().any(|note| note.kind == NoteKind::Unknown && note.text == unknown));
        assert_eq!(s.app.config.sidebar, "tree");
        assert!(described.notices.iter().any(|notice| notice == unknown), "{:?}", described.notices);
    }

    #[test]
    fn an_unknown_section_is_refused_by_name() {
        let mut s = setup();

        let refusal = refused(&mut s.app, "config-import", &json!({"text": "version = 1\n", "sections": ["look"]}));

        assert_eq!(refusal.code, "malformed");
        assert_eq!(refusal.message, "`look` is not a section; use: agents, issues, worktrees, interface, repositories");
    }
}

mod refusing_changes_nothing {
    use super::*;

    const TWO_ENTRIES: &str = "version = 1\n\
        [[repositories]]\nremotes = [\"github.com/acme/web-shop\"]\nsetup = \"npm ci\"\nteardown = \"\"\n\
        [[repositories]]\nremotes = [\"github.com/up/web-shop\"]\nsetup = \"yarn\"\nteardown = \"\"\n";

    #[rstest]
    #[case::newer_version("version = 2\n")]
    #[case::no_version("[interface]\nsidebar = \"tree\"\n")]
    #[case::bad_choice("version = 1\n[interface]\nsidebar = \"diagonal\"\n")]
    #[case::relative_worktrees_dir("version = 1\n[worktrees]\nworktrees_dir = \"rel\"\n")]
    #[case::wrong_type("version = 1\n[agents]\nagent_args = 3\n")]
    #[case::two_entries_one_project(TWO_ENTRIES)]
    fn and_says_why(#[case] text: &str) {
        let mut s = setup();
        let _repo = open_clone(&mut s, &[("origin", SHOP_URL), ("upstream", "https://github.com/up/web-shop")]);
        set(&mut s, &json!({"counts": false}));
        let before = bytes(&s);

        let previewing = refused(&mut s.app, "config-import", &import_args(text, None, false));
        let applying = refused(&mut s.app, "config-import", &import_args(text, None, true));

        assert_eq!((previewing.code.as_str(), applying.code.as_str()), ("malformed", "malformed"), "{applying:?}");
        assert_eq!(bytes(&s), before);
    }

    #[test]
    fn two_entries_for_one_project_name_both_remotes() {
        let mut s = setup();
        let _repo = open_clone(&mut s, &[("origin", SHOP_URL), ("upstream", "https://github.com/up/web-shop")]);

        let refusal = refused(&mut s.app, "config-import", &import_args(TWO_ENTRIES, None, true));

        assert!(refusal.message.contains("github.com/acme/web-shop and github.com/up/web-shop"), "{refusal:?}");
    }

    #[test]
    fn a_file_over_the_limit() {
        let mut s = setup();
        let text = format!("version = 1\n#{}\n", "x".repeat(share::MAX_BYTES));

        let refusal = refused(&mut s.app, "config-import", &import_args(&text, None, true));

        assert_eq!((refusal.code.as_str(), refusal.message), ("malformed", share::too_large()));
    }

    #[test]
    fn an_unreadable_config_refuses_with_conflict() {
        let mut s = setup();
        std::fs::write(s.config.path().join("config.json"), "{").expect("write");

        let refusal = refused(&mut s.app, "config-import", &import_args(&shop_setup("make"), None, true));

        assert_eq!(refusal.code, "conflict");
        assert_eq!(bytes(&s), Some(b"{".to_vec()));
        assert!(export(&mut s).text.contains("version = 1"));
    }
}
