use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::test_util::TempDir;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, "x").expect("write");
}

fn claude_home(dir: &TempDir) -> PathBuf {
    let home = dir.path().join("default");
    for file in ["settings.json", "CLAUDE.md", "statusline.sh", ".credentials.json", ".claude.json", "history.jsonl"] {
        touch(&home.join(file));
    }
    for folder in ["skills", "agents", "projects", "sessions"] {
        fs::create_dir_all(home.join(folder)).expect("create folder");
    }
    home
}

fn claude_work(dir: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let home = claude_home(dir);
    let root = dir.path().join("accounts");
    let account = add(&root, Known::Claude, "work", Some(&home)).expect("add");
    (home, root, account)
}

mod names {
    use super::*;

    #[rstest::rstest]
    #[case::word("work")]
    #[case::symbols("me.personal_2-b")]
    #[case::longest("abcdefghijabcdefghijabcdefghijab")]
    fn are_accepted(#[case] name: &str) {
        assert!(valid(name));
    }

    #[rstest::rstest]
    #[case::empty("")]
    #[case::dot(".")]
    #[case::dots("..")]
    #[case::slash("a/b")]
    #[case::space("my work")]
    #[case::accent("trabajó")]
    #[case::too_long("abcdefghijabcdefghijabcdefghijabc")]
    fn are_refused(#[case] name: &str) {
        assert!(!valid(name));
    }

    #[test]
    fn a_refused_name_creates_nothing() {
        let dir = TempDir::new();
        let root = dir.path().join("accounts");
        let error = add(&root, Known::Claude, "../evil", None).expect_err("refused");
        assert!(error.to_string().contains("not a valid account name"), "{error}");
        assert!(!root.exists());
    }
}

mod adding {
    use super::*;

    #[test]
    fn makes_a_private_folder_with_links_to_every_listed_entry_and_no_other() {
        let dir = TempDir::new();
        let (home, root, account) = claude_work(&dir);
        assert_eq!(account, root.join("claude").join("work"));
        assert_eq!((mode(&root), mode(&root.join("claude")), mode(&account)), (0o700, 0o700, 0o700));
        let mut linked: Vec<(String, PathBuf)> = fs::read_dir(&account)
            .expect("read")
            .map(|entry| entry.expect("entry"))
            .map(|entry| {
                (entry.file_name().into_string().expect("utf-8"), fs::read_link(entry.path()).expect("a link"))
            })
            .collect();
        linked.sort();
        let expected: Vec<(String, PathBuf)> = ["CLAUDE.md", "agents", "settings.json", "skills", "statusline.sh"]
            .into_iter()
            .map(|name| (name.to_string(), home.join(name)))
            .collect();
        assert_eq!(linked, expected);
    }

    #[test]
    fn links_gemini_settings_inside_its_own_folder() {
        let dir = TempDir::new();
        let home = dir.path().join("default");
        touch(&home.join(".gemini").join("settings.json"));
        touch(&home.join(".gemini").join("oauth_creds.json"));
        let account = add(&dir.path().join("accounts"), Known::Gemini, "work", Some(&home)).expect("add");
        let gemini = account.join(".gemini");
        assert_eq!(mode(&gemini), 0o700);
        assert_eq!(
            fs::read_link(gemini.join("settings.json")).expect("link"),
            home.join(".gemini").join("settings.json")
        );
        assert!(!gemini.join("oauth_creds.json").exists());
    }

    #[test]
    fn refuses_an_account_that_exists() {
        let dir = TempDir::new();
        let root = dir.path().join("accounts");
        add(&root, Known::Codex, "work", None).expect("add");
        let error = add(&root, Known::Codex, "work", None).expect_err("refused");
        assert_eq!(error.to_string(), "the codex account `work` already exists");
    }

    #[test]
    fn a_link_that_cannot_be_made_leaves_no_folder_so_adding_again_works() {
        let dir = TempDir::new();
        let home = dir.path().join("default");
        touch(&home.join(format!("statusline-{}", "x".repeat(200))));
        let limit = if cfg!(target_os = "macos") { 1024 } else { 4096 };
        let mut root = dir.path().canonicalize().expect("canonical");
        while root.as_os_str().len() + "/claude/work".len() < limit - 80 {
            root.push("d".repeat(60));
        }

        let error = add(&root, Known::Claude, "work", Some(&home)).expect_err("the link is too long");

        assert!(error.to_string().starts_with("could not link the settings into"), "{error}");
        assert!(!root.join("claude").join("work").exists());
        fs::remove_file(home.join(format!("statusline-{}", "x".repeat(200)))).expect("remove the entry");
        add(&root, Known::Claude, "work", Some(&home)).expect("added the second time");
    }

    #[rstest::rstest]
    #[case::opencode(Known::Opencode)]
    #[case::copilot(Known::Copilot)]
    #[case::cursor(Known::Cursor)]
    #[case::grok(Known::Grok)]
    fn refuses_an_agent_without_accounts(#[case] kind: Known) {
        let dir = TempDir::new();
        let root = dir.path().join("accounts");
        assert!(add(&root, kind, "work", None).is_err());
        assert!(!root.exists());
    }

    #[test]
    fn only_claude_codex_and_gemini_have_accounts() {
        assert_eq!(supported().collect::<Vec<_>>(), [Known::Claude, Known::Codex, Known::Gemini]);
    }
}

mod listing {
    use super::*;

    #[test]
    fn reads_the_folders_on_disk_by_agent_in_order() {
        let dir = TempDir::new();
        let root = dir.path().join("accounts");
        for name in ["work", "personal"] {
            add(&root, Known::Claude, name, None).expect("add");
        }
        add(&root, Known::Gemini, "solo", None).expect("add");
        touch(&root.join("claude").join("stray-file"));
        fs::create_dir_all(root.join("claude").join("not valid")).expect("create");
        assert_eq!(
            list(&root),
            vec![
                (Known::Claude, vec!["personal".to_string(), "work".to_string()]),
                (Known::Codex, Vec::new()),
                (Known::Gemini, vec!["solo".to_string()]),
            ]
        );
    }
}

mod removing {
    use super::*;

    #[test]
    fn deletes_the_folder_but_not_what_its_links_point_to() {
        let dir = TempDir::new();
        let (home, root, account) = claude_work(&dir);
        remove(&root, Known::Claude, "work").expect("remove");
        assert!(!account.exists());
        assert!(home.join("settings.json").exists() && home.join("skills").is_dir());
    }

    #[test]
    fn says_when_there_is_no_such_account() {
        let dir = TempDir::new();
        let error = remove(dir.path(), Known::Codex, "ghost").expect_err("refused");
        assert_eq!(error.to_string(), "there is no codex account `ghost`");
    }

    #[test]
    fn never_runs_the_keychain_tool() {
        let needle = ["\"", "secur", "ity", "\""].concat();
        let mut folders = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("accounts")];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).expect("read src/accounts") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    folders.push(path);
                    continue;
                }
                let text = fs::read_to_string(&path).expect("read");
                assert!(!text.contains(&needle), "{} names the keychain tool", path.display());
            }
        }
    }
}

mod sharing {
    use super::*;

    #[test]
    fn a_real_file_is_never_replaced_and_is_reported() {
        let dir = TempDir::new();
        let (home, _, account) = claude_work(&dir);
        fs::remove_file(account.join("settings.json")).expect("remove link");
        fs::write(account.join("settings.json"), "mine").expect("write");
        assert_eq!(links::ensure(Known::Claude, &home, &account).expect("ensure"), 0);
        assert_eq!(fs::read_to_string(account.join("settings.json")).expect("read"), "mine");
        assert_eq!(links::status(Known::Claude, &home, &account), ["settings.json"]);
    }

    #[test]
    fn missing_links_are_made_again() {
        let dir = TempDir::new();
        let (home, _, account) = claude_work(&dir);
        touch(&home.join("keybindings.json"));
        fs::remove_file(account.join("CLAUDE.md")).expect("remove link");
        assert_eq!(links::ensure(Known::Claude, &home, &account).expect("ensure"), 2);
        assert_eq!(fs::read_link(account.join("keybindings.json")).expect("link"), home.join("keybindings.json"));
        assert_eq!(links::status(Known::Claude, &home, &account), Vec::<String>::new());
    }

    #[test]
    fn the_default_home_is_never_linked_into_itself() {
        let dir = TempDir::new();
        let home = claude_home(&dir);
        assert_eq!(links::ensure(Known::Claude, &home, &home).expect("ensure"), 0);
        assert!(fs::symlink_metadata(home.join("settings.json")).is_ok_and(|meta| meta.is_file()));
    }
}

mod default_homes {
    use std::ffi::OsString;

    use super::*;

    fn home(kind: Known, vars: &[(&str, &str)]) -> Option<PathBuf> {
        let account = kind.account().expect("has accounts");
        account.default_home(|name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| OsString::from(value)))
    }

    #[rstest::rstest]
    #[case::claude(Known::Claude, &[("HOME", "/h")], Some("/h/.claude"))]
    #[case::claude_variable(Known::Claude, &[("HOME", "/h"), ("CLAUDE_CONFIG_DIR", "/c")], Some("/c"))]
    #[case::codex(Known::Codex, &[("HOME", "/h")], Some("/h/.codex"))]
    #[case::codex_variable(Known::Codex, &[("HOME", "/h"), ("CODEX_HOME", "/x")], Some("/x"))]
    #[case::gemini(Known::Gemini, &[("HOME", "/h")], Some("/h"))]
    #[case::gemini_variable(Known::Gemini, &[("HOME", "/h"), ("GEMINI_CLI_HOME", "/g")], Some("/g"))]
    #[case::relative_is_ignored(Known::Codex, &[("HOME", "/h"), ("CODEX_HOME", "x")], Some("/h/.codex"))]
    #[case::no_home(Known::Codex, &[], None)]
    fn follow_the_variable_then_home(
        #[case] kind: Known,
        #[case] vars: &[(&str, &str)],
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(home(kind, vars), expected.map(PathBuf::from));
    }
}

mod applied {
    use super::*;

    pub(super) fn chosen(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(p, a)| ((*p).to_string(), (*a).to_string())).collect()
    }

    #[test]
    fn a_profile_gives_each_agent_its_canonical_home_in_agent_order() {
        let dir = TempDir::new();
        let claude = add(dir.path(), Known::Claude, "work", None).expect("claude");
        let codex = add(dir.path(), Known::Codex, "me", None).expect("codex");

        let accounts = Accounts::of(dir.path(), &chosen(&[("codex", "me"), ("claude", "work")]));

        assert_eq!(
            accounts.env(),
            [
                ("CLAUDE_CONFIG_DIR".to_string(), claude.display().to_string()),
                ("CODEX_HOME".to_string(), codex.display().to_string()),
            ]
        );
        assert_eq!(accounts.removed(), ["CLAUDE_SECURESTORAGE_CONFIG_DIR"]);
        assert_eq!((accounts.name(Known::Codex), accounts.name(Known::Gemini)), (Some("me"), None));
    }

    #[test]
    fn missing_folders_unknown_agents_and_bad_names_are_left_out() {
        let dir = TempDir::new();
        add(dir.path(), Known::Codex, "me", None).expect("codex");

        let accounts = Accounts::of(
            dir.path(),
            &chosen(&[("claude", "gone"), ("opencode", "x"), ("nobody", "x"), ("codex", "..")]),
        );

        assert!(accounts.is_empty());
    }

    #[test]
    fn only_a_claude_account_removes_the_secure_storage_variable() {
        let dir = TempDir::new();
        add(dir.path(), Known::Gemini, "me", None).expect("gemini");

        assert_eq!(Accounts::of(dir.path(), &chosen(&[("gemini", "me")])).removed(), [] as [&str; 0]);
    }
}

mod seen {
    use rstest::rstest;

    use super::*;

    fn var(dir: &str) -> Vec<(String, String)> {
        vec![("PATH".to_string(), "/bin".to_string()), ("CLAUDE_CONFIG_DIR".to_string(), dir.to_string())]
    }

    fn seen(root: &Path, given: &Accounts, env: Vec<(String, String)>) -> Option<String> {
        let mut seen = Seen::default();
        seen.update(root, given, Some((7, Known::Claude, &[])), |_| env);
        seen.name().map(str::to_string)
    }

    #[rstest]
    #[case::an_account(&["claude", "work"], Some("work"))]
    #[case::the_provider_folder(&["claude"], Some(CUSTOM))]
    #[case::inside_an_account(&["claude", "work", "deeper"], Some(CUSTOM))]
    #[case::another_provider_s_account(&["codex", "work"], Some(CUSTOM))]
    fn a_folder_names_its_account(#[case] parts: &[&str], #[case] expected: Option<&str>) {
        let dir = TempDir::new();
        let root = dir.path().join("accounts");
        let folder = parts.iter().fold(root.clone(), |path, part| path.join(part));
        fs::create_dir_all(&folder).expect("the folder");

        let found = seen(&root, &Accounts::default(), var(folder.to_str().expect("utf-8")));

        assert_eq!(found.as_deref(), expected);
    }

    #[rstest]
    #[case::elsewhere("/opt/claude", Some(CUSTOM))]
    #[case::relative("claude", None)]
    #[case::empty("", None)]
    fn a_variable_outside_the_accounts_is_custom_or_nothing(#[case] dir: &str, #[case] expected: Option<&str>) {
        let root = TempDir::new();

        assert_eq!(seen(root.path(), &Accounts::default(), var(dir)).as_deref(), expected);
    }

    #[test]
    fn without_the_variable_there_is_no_account() {
        let root = TempDir::new();

        assert_eq!(seen(root.path(), &Accounts::default(), vec![("PATH".into(), "/bin".into())]), None);
    }

    #[test]
    fn the_account_cornercase_gave_the_pane_wins_and_the_process_is_not_read() {
        let dir = TempDir::new();
        add(dir.path(), Known::Claude, "work", None).expect("claude");
        let given = Accounts::of(dir.path(), &applied::chosen(&[("claude", "work")]));
        let mut seen = Seen::default();

        seen.update(dir.path(), &given, Some((7, Known::Claude, &[])), |_| panic!("the process was read"));

        assert_eq!(seen.name(), Some("work"));
    }

    #[test]
    fn the_process_is_read_once_until_it_changes() {
        let root = TempDir::new();
        let (mut seen, reads) = (Seen::default(), std::cell::Cell::new(0));
        let mut look = |pid, args: &[String]| {
            seen.update(root.path(), &Accounts::default(), Some((pid, Known::Claude, args)), |_| {
                reads.set(reads.get() + 1);
                var("/opt/claude")
            });
        };

        look(7, &[]);
        look(7, &[]);
        look(7, &["claude".to_string()]);
        look(8, &["claude".to_string()]);

        assert_eq!(reads.get(), 3);
    }

    #[test]
    fn no_agent_forgets_the_account() {
        let root = TempDir::new();
        let mut seen = Seen::default();
        seen.update(root.path(), &Accounts::default(), Some((7, Known::Claude, &[])), |_| var("/opt/claude"));

        seen.update(root.path(), &Accounts::default(), None, |_| panic!("nothing to read"));

        assert_eq!(seen.name(), None);
    }
}

mod login_status {
    use std::ffi::OsString;

    use super::*;
    use crate::accounts::login::{self, Login, State};
    use crate::test_util::write_executable;

    const SIGNED_IN: &str = r#"{"loggedIn": true, "authMethod": "claude.ai", "email": "me@work.dev",
        "orgName": "Acme", "subscriptionType": "max"}"#;

    #[test]
    fn claude_s_answer_gives_the_email_and_the_plan() {
        let found = Known::Claude.login(SIGNED_IN);

        assert_eq!(
            found,
            Login { state: State::SignedIn, email: Some("me@work.dev".into()), plan: Some("max".into()) }
        );
        assert_eq!(found.text(), "signed in as me@work.dev (max)");
    }

    #[test]
    fn claude_signed_out_and_an_answer_it_cannot_read_are_told_apart() {
        let out = r#"{"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty"}"#;

        assert_eq!(Known::Claude.login(out), Login::of(State::SignedOut));
        assert_eq!(Known::Claude.login("error: unknown command"), Login::of(State::Unknown));
    }

    #[test]
    fn codex_s_status_line_gives_the_state_and_how() {
        let chatgpt = Known::Codex.login("Logged in using ChatGPT\n");
        let key = Known::Codex.login("Logged in using an API key - sk-***abcd\n");

        assert_eq!((chatgpt.state, chatgpt.plan.as_deref()), (State::SignedIn, Some("chatgpt")));
        assert_eq!((key.state, key.plan.as_deref(), key.email), (State::SignedIn, Some("api key"), None));
        assert_eq!(Known::Codex.login("Not logged in\n"), Login::of(State::SignedOut));
        assert_eq!(Known::Gemini.login("anything"), Login::of(State::Unknown));
    }

    #[test]
    fn only_the_names_of_the_token_variables_that_are_set_are_given() {
        let env = |name: &str| match name {
            "ANTHROPIC_API_KEY" => Some(OsString::from("sk-secret")),
            "OPENAI_API_KEY" => Some(OsString::new()),
            _ => None,
        };

        assert_eq!(login::tokens(env), ["ANTHROPIC_API_KEY"]);
    }

    #[test]
    fn gemini_s_encrypted_storage_switch_is_no_token() {
        let env = |name: &str| (name == "GEMINI_FORCE_ENCRYPTED_FILE_STORAGE").then(|| OsString::from("true"));

        assert_eq!(login::tokens(env), Vec::<String>::new());
    }

    #[test]
    fn the_check_drops_the_session_variables_of_an_agent_the_server_runs_in() {
        let dir = TempDir::new();

        let command = login::command(Known::Claude, "claude", dir.path()).expect("claude has a status command");

        let removed: Vec<_> = command.get_envs().filter(|(_, value)| value.is_none()).map(|(key, _)| key).collect();
        for name in crate::agents::claude::CLAUDE_SESSION_ENV.iter().chain(&crate::agents::copilot::SESSION_ENV) {
            assert!(removed.contains(&std::ffi::OsStr::new(name)), "{name} is removed");
        }
        assert!(removed.contains(&std::ffi::OsStr::new("CLAUDE_SECURESTORAGE_CONFIG_DIR")));
    }

    #[test]
    fn the_check_runs_the_agent_with_the_account_s_folder_and_reads_nothing_in_it() {
        let dir = TempDir::new();
        let account = dir.path().join("work");
        fs::create_dir_all(&account).expect("the account");
        let secret = account.join(".credentials.json");
        fs::write(&secret, "secret").expect("credentials");
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).expect("chmod");
        let out = dir.path().join("out");
        let fake = dir.path().join("claude");
        let script = format!(
            "#!/bin/sh\nprintf '%s|%s|%s' \"$*\" \"$CLAUDE_CONFIG_DIR\" \"$PWD\" > {}\necho '{}'\nexit 1\n",
            out.display(),
            SIGNED_IN.replace('\n', " ")
        );
        write_executable(&fake, &script);

        let found = login::check(Known::Claude, &fake.display().to_string(), &account);

        assert_eq!(found.state, State::SignedIn);
        let home = account.display().to_string();
        let pwd = account.canonicalize().expect("canonical").display().to_string();
        assert_eq!(fs::read_to_string(&out).expect("recorded"), format!("auth status --json|{home}|{pwd}"));
        let source =
            fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/accounts/login.rs")).expect("login.rs");
        assert!(!source.contains(".credentials") && !source.contains("auth.json"));
    }

    #[test]
    fn the_system_login_is_checked_with_the_server_s_own_variable() {
        let dir = TempDir::new();

        let system = login::system(Known::Claude, "claude", Some(dir.path())).expect("claude has a status command");
        let account = login::command(Known::Claude, "claude", dir.path()).expect("claude has a status command");

        let set = |command: &std::process::Command| {
            command.get_envs().any(|(key, value)| key == "CLAUDE_CONFIG_DIR" && value.is_some())
        };
        assert_eq!((set(&system), set(&account)), (false, true));
        assert_eq!(system.get_current_dir(), Some(dir.path()));
        let missing = dir.path().join("missing");
        assert_eq!(
            login::system(Known::Claude, "claude", Some(&missing))
                .and_then(|c| c.get_current_dir().map(Path::to_path_buf)),
            None
        );
    }

    #[test]
    fn a_check_that_cannot_run_failed() {
        let dir = TempDir::new();

        let found = login::check(Known::Codex, "/nonexistent/codex", dir.path());

        assert_eq!(found, Login::of(State::Failed));
        assert_eq!(login::check(Known::Gemini, "/nonexistent/gemini", dir.path()), Login::default());
    }
}

mod credentials {
    #[test]
    fn the_accounts_code_never_runs_the_keychain_tool_or_opens_a_credentials_file() {
        for (name, source) in [
            ("mod.rs", include_str!("mod.rs")),
            ("links.rs", include_str!("links.rs")),
            ("login.rs", include_str!("login.rs")),
            ("pick.rs", include_str!("pick.rs")),
        ] {
            let production = source.split("#[cfg(test)]").next().expect("source");
            for forbidden in ["Command::new(\"security\"", "\"find-generic-password\"", ".credentials.json"] {
                assert!(!production.contains(forbidden), "{name} mentions {forbidden}");
            }
        }
    }
}
