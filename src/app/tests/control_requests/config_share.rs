use serde_json::json;

use super::config::{MARKER, Setup, ok as configured, set, setup, shortcut};
use super::*;
use crate::control::config::SetupExported;
use crate::settings::share::{self, Repository, Shared};
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
