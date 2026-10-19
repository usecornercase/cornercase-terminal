use std::collections::BTreeMap;

use super::*;
use crate::accounts::login::State;
use crate::control::accounts::{AccountInfo, AccountsInfo, LoginInfo, ProviderInfo};
use crate::control::{AccountAdd, AccountRename, AccountUse};

fn account(name: &str, login: Option<LoginInfo>, unshared: &[&str]) -> AccountInfo {
    AccountInfo {
        name: name.into(),
        path: PathBuf::from(format!("/home/ana/.local/state/cornercase/accounts/claude/{name}")),
        unshared: unshared.iter().map(ToString::to_string).collect(),
        login,
        ..AccountInfo::default()
    }
}

fn signed_in(email: &str) -> LoginInfo {
    LoginInfo { state: State::SignedIn, email: Some(email.into()), plan: Some("max".into()), checked: String::new() }
}

fn with_accounts() -> Settings {
    let mut s = settings();
    let claude = ProviderInfo {
        provider: "claude".into(),
        title: "Claude Code".into(),
        env: "CLAUDE_CONFIG_DIR".into(),
        accounts: vec![
            account("home", None, &["settings.json"]),
            account("work", Some(signed_in("ana@acme.dev")), &[]),
        ],
        ..ProviderInfo::default()
    };
    s.accounts.info = AccountsInfo { providers: vec![claude], ..AccountsInfo::default() };
    s.config.profiles =
        BTreeMap::from([("Work".to_string(), BTreeMap::from([("claude".to_string(), "work".to_string())]))]);
    s.config.profile = "Work".into();
    s.open_page(Page::Accounts);
    s
}

fn text(s: &Settings, row: &Row) -> (String, String, String) {
    let text = s.row_text(row);
    (text.label, text.value, text.note)
}

fn order(action: Action) -> Order {
    match action {
        Action::Accounts(order) => order,
        other => panic!("no order: {other:?}"),
    }
}

fn rename(account: &str, label: &str) -> Order {
    Order::Rename(AccountRename { provider: "claude".into(), account: account.into(), label: label.into() })
}

mod rows {
    use super::*;

    #[test]
    fn list_each_agent_s_own_login_then_its_accounts_with_no_profiles() {
        let s = with_accounts();
        assert_eq!(
            s.rows(),
            [
                Row::System(Known::Claude),
                Row::RenameAccount(Known::Claude, String::new()),
                Row::Account(Known::Claude, "home".into()),
                Row::RenameAccount(Known::Claude, "home".into()),
                Row::Account(Known::Claude, "work".into()),
                Row::RenameAccount(Known::Claude, "work".into()),
                Row::AddAccount(Known::Claude),
                Row::System(Known::Codex),
                Row::RenameAccount(Known::Codex, String::new()),
                Row::AddAccount(Known::Codex),
                Row::System(Known::Gemini),
                Row::RenameAccount(Known::Gemini, String::new()),
                Row::AddAccount(Known::Gemini),
            ]
        );
    }

    #[test]
    fn warn_first_about_tokens_set_in_the_server() {
        let mut s = with_accounts();
        s.accounts.info.tokens = vec!["ANTHROPIC_API_KEY".into()];
        assert_eq!(s.rows()[0], Row::ServerTokens);
        let shown = s.row_text(&Row::ServerTokens);
        assert_eq!(
            (shown.value.as_str(), shown.note.as_str(), shown.dangerous),
            ("ANTHROPIC_API_KEY", "it wins over the account", true)
        );
    }

    #[test]
    fn are_left_out_of_the_config_request() {
        let s = with_accounts();
        assert!(s.describe().iter().all(|setting| setting.page != "Accounts"));
    }
}

mod texts {
    use super::*;

    #[test]
    fn an_account_shows_its_login_and_its_folder() {
        let s = with_accounts();
        assert_eq!(
            text(&s, &Row::Account(Known::Claude, "work".into())),
            (
                "● work".into(),
                "signed in as ana@acme.dev (max)".into(),
                "~/.local/state/cornercase/accounts/claude/work".into()
            )
        );
    }

    #[test]
    fn the_system_login_shows_its_name_its_folder_and_its_login() {
        let mut s = with_accounts();
        let claude = &mut s.accounts.info.providers[0];
        claude.system_path = Some(PathBuf::from("/home/ana/.claude"));
        claude.system_login = Some(signed_in("ana@home.dev"));
        s.config.account_names = BTreeMap::from([(
            "claude".to_string(),
            BTreeMap::from([(String::new(), "Office".to_string()), ("home".to_string(), "Personal".to_string())]),
        )]);
        assert_eq!(
            text(&s, &Row::System(Known::Claude)),
            ("  Office".into(), "signed in as ana@home.dev (max)".into(), "claude's own login · ~/.claude".into())
        );
        assert_eq!(s.row_text(&Row::Account(Known::Claude, "home".into())).label, "  Personal");
        assert_eq!(
            text(&s, &Row::System(Known::Codex)),
            ("● System".into(), String::new(), "codex's own login".into())
        );
    }

    #[test]
    fn an_account_being_checked_says_so() {
        let mut s = with_accounts();
        s.accounts.checking = vec![(Known::Claude, "work".into()), (Known::Claude, String::new())];
        assert_eq!(s.row_text(&Row::Account(Known::Claude, "work".into())).value, "checking…");
        assert_eq!(s.row_text(&Row::System(Known::Claude)).value, "checking…");
    }

    #[test]
    fn a_setting_that_is_no_longer_shared_is_named() {
        let s = with_accounts();
        let shown = s.row_text(&Row::Account(Known::Claude, "home".into()));
        assert_eq!((shown.value.as_str(), shown.dangerous), ("", true));
        assert!(shown.note.ends_with(" · not shared: settings.json"), "{}", shown.note);
    }

    #[test]
    fn the_account_in_use_is_marked_whether_a_profile_or_the_window_chose_it() {
        let mut s = with_accounts();
        let marked = |s: &Settings| {
            s.rows().iter().filter(|row| s.row_text(row).label.starts_with('●')).cloned().collect::<Vec<_>>()
        };
        assert!(marked(&s).contains(&Row::Account(Known::Claude, "work".into())));
        let chosen = BTreeMap::from([("claude".to_string(), "home".to_string())]);
        s.take_config(Config { profile: String::new(), active_accounts: chosen, ..s.config.clone() });
        assert_eq!(
            marked(&s),
            [Row::Account(Known::Claude, "home".into()), Row::System(Known::Codex), Row::System(Known::Gemini)]
        );
    }

    #[test]
    fn an_account_that_is_gone_leaves_the_system_login_in_use() {
        let mut s = with_accounts();
        let chosen = BTreeMap::from([("claude".to_string(), "old".to_string())]);
        s.take_config(Config { profile: String::new(), active_accounts: chosen, ..s.config.clone() });
        assert_eq!(s.row_text(&Row::System(Known::Claude)).label, "● System");
    }
}

mod actions {
    use super::*;

    #[test]
    fn enter_uses_an_account_or_the_system_login() {
        let mut s = with_accounts();
        go_to(&mut s, &Row::Account(Known::Claude, "home".into()));
        let home = AccountUse { provider: "claude".into(), account: "home".into() };
        assert_eq!(order(press(&mut s, KeyCode::Enter)), Order::Use(home));
        go_to(&mut s, &Row::System(Known::Codex));
        let system = AccountUse { provider: "codex".into(), account: String::new() };
        assert_eq!(order(press(&mut s, KeyCode::Enter)), Order::Use(system));
    }

    #[test]
    fn the_system_login_cannot_be_removed() {
        let mut s = with_accounts();
        go_to(&mut s, &Row::System(Known::Claude));
        assert_eq!(press(&mut s, KeyCode::Backspace), Action::None);
    }

    #[test]
    fn backspace_on_an_account_asks_first() {
        let mut s = with_accounts();
        go_to(&mut s, &Row::Account(Known::Claude, "home".into()));
        assert_eq!(press(&mut s, KeyCode::Backspace), Action::RemoveAccount(Known::Claude, "home".into()));
    }

    #[test]
    fn a_new_account_opens_its_login_tab() {
        let mut s = with_accounts();
        go_to(&mut s, &Row::AddAccount(Known::Codex));
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "work");
        assert_eq!(
            order(press(&mut s, KeyCode::Enter)),
            Order::Add(AccountAdd {
                provider: "codex".into(),
                name: "work".into(),
                login: true,
                workspace: None,
                focus: true
            })
        );
    }

    #[rstest::rstest]
    #[case::taken("work", "the claude account `work` already exists")]
    #[case::invalid("my work", "use letters, digits, `.`, `_` and `-`, at most 32 characters")]
    #[case::empty("", "type a name first")]
    fn a_bad_account_name_stays_in_the_field(#[case] name: &str, #[case] error: &str) {
        let mut s = with_accounts();
        go_to(&mut s, &Row::AddAccount(Known::Claude));
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, name);
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert_eq!(s.edit.and_then(|edit| edit.error).as_deref(), Some(error));
    }

    #[test]
    fn a_taken_name_says_how_the_window_shows_that_account() {
        let mut s = with_accounts();
        s.config.account_names =
            BTreeMap::from([("claude".to_string(), BTreeMap::from([("home".to_string(), "work".to_string())]))]);
        go_to(&mut s, &Row::AddAccount(Known::Claude));
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "home");
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert_eq!(
            s.edit.and_then(|edit| edit.error).as_deref(),
            Some("the claude account `home` already exists (shown as `work`)")
        );
    }

    #[test]
    fn a_new_folder_cannot_take_a_name_the_window_already_shows() {
        let mut s = with_accounts();
        s.config.account_names =
            BTreeMap::from([("claude".to_string(), BTreeMap::from([(String::new(), "Office".to_string())]))]);
        go_to(&mut s, &Row::AddAccount(Known::Claude));
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "office");
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert_eq!(
            s.edit.and_then(|edit| edit.error).as_deref(),
            Some("another claude account is already called `Office`")
        );
    }

    #[test]
    fn a_named_account_shows_its_folder() {
        let mut s = with_accounts();
        s.config.account_names =
            BTreeMap::from([("claude".to_string(), BTreeMap::from([("home".to_string(), "Personal".to_string())]))]);
        let (_, _, note) = text(&s, &Row::Account(Known::Claude, "home".into()));
        assert!(note.starts_with("folder home · "), "{note}");
        let (_, _, note) = text(&s, &Row::Account(Known::Claude, "work".into()));
        assert!(!note.contains("folder"), "{note}");
    }

    #[test]
    fn rename_starts_from_the_saved_name_and_sends_the_new_one() {
        let mut s = with_accounts();
        s.config.account_names =
            BTreeMap::from([("claude".to_string(), BTreeMap::from([("home".to_string(), "Home".to_string())]))]);
        go_to(&mut s, &Row::RenameAccount(Known::Claude, "home".into()));
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.edit.as_ref().map(|edit| edit.input.as_str()), Some("Home"));
        type_text(&mut s, "s ");
        assert_eq!(order(press(&mut s, KeyCode::Enter)), rename("home", "Homes"));
    }

    #[test]
    fn the_system_login_is_renamed_from_an_empty_field() {
        let mut s = with_accounts();
        go_to(&mut s, &Row::RenameAccount(Known::Claude, String::new()));
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.edit.as_ref().map(|edit| edit.input.as_str()), Some(""));
        type_text(&mut s, "work");
        assert_eq!(order(press(&mut s, KeyCode::Enter)), rename("", "work"));
    }

    #[test]
    fn an_empty_name_clears_the_saved_one() {
        let mut s = with_accounts();
        go_to(&mut s, &Row::RenameAccount(Known::Claude, "work".into()));
        press(&mut s, KeyCode::Enter);
        assert_eq!(order(press(&mut s, KeyCode::Enter)), rename("work", ""));
    }
}
