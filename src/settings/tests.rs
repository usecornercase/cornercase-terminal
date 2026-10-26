use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::*;

fn settings() -> Settings {
    Settings::new(
        Config::default(),
        Some(PathBuf::from("/home/ana")),
        vec![(Source::Shortcut, Status::Missing), (Source::Linear, Status::Env)],
        Page::default(),
    )
}

fn key(s: &mut Settings, code: KeyCode, modifiers: KeyModifiers) -> Action {
    s.key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE }, 20)
}

fn press(s: &mut Settings, code: KeyCode) -> Action {
    key(s, code, KeyModifiers::NONE)
}

fn type_text(s: &mut Settings, text: &str) {
    for c in text.chars() {
        press(s, KeyCode::Char(c));
    }
}

fn go_to(s: &mut Settings, row: &Row) {
    s.open_page(row.page());
    s.cursor = s.rows().iter().position(|r| r == row).expect("the row is there");
}

fn saved(action: Action) -> Config {
    match action {
        Action::Save(config) => *config,
        other => panic!("nothing saved: {other:?}"),
    }
}

mod pages {
    use rstest::rstest;

    use super::*;

    fn sections(page: Page) -> Vec<&'static str> {
        let mut s = settings();
        s.open_page(page);
        let mut sections: Vec<&str> = s.rows().iter().map(Row::section).collect();
        sections.dedup();
        sections
    }

    #[rstest]
    #[case::worktrees(Page::Worktrees, &[""])]
    #[case::agents(Page::Agents, &["Agent", "How each agent starts"])]
    #[case::accounts(Page::Accounts, &["Claude Code", "Codex", "Gemini CLI"])]
    #[case::flows(Page::Flows, &[])]
    #[case::issues(Page::Issues, &["Accounts", "Sources shown"])]
    #[case::ui(Page::Ui, &[""])]
    #[case::remote(Page::Remote, &["Browser", "Telegram", "Discord"])]
    fn hold_their_sections(#[case] page: Page, #[case] expected: &[&str]) {
        assert_eq!(sections(page), expected);
    }

    #[test]
    fn the_first_one_is_worktrees() {
        assert_eq!(
            settings().rows(),
            [Row::Folder, Row::IssueBranch, Row::Fetch, Row::PullRequests, Row::MergedPullRequests]
        );
    }

    #[rstest]
    #[case::tab(KeyCode::Tab, KeyModifiers::NONE, Page::Agents)]
    #[case::right(KeyCode::Right, KeyModifiers::NONE, Page::Agents)]
    #[case::shift_tab_wraps(KeyCode::BackTab, KeyModifiers::SHIFT, Page::Plugins)]
    #[case::left_wraps(KeyCode::Left, KeyModifiers::NONE, Page::Plugins)]
    fn are_switched_with_keys(#[case] code: KeyCode, #[case] modifiers: KeyModifiers, #[case] expected: Page) {
        let mut s = settings();
        key(&mut s, code, modifiers);
        assert_eq!(s.page, expected);
    }

    #[test]
    fn the_flows_tab_comes_after_accounts_and_has_no_rows() {
        let mut s = settings();
        s.open_page(Page::Accounts);
        press(&mut s, KeyCode::Right);
        assert_eq!(s.page, Page::Flows);
        assert_eq!(s.rows().len(), 0);
        press(&mut s, KeyCode::Right);
        assert_eq!(s.page, Page::Issues);
    }

    #[test]
    fn switching_selects_the_first_row() {
        let mut s = settings();
        go_to(&mut s, &Row::Trust);
        press(&mut s, KeyCode::Tab);
        assert_eq!((s.page, s.cursor), (Page::Accounts, 0));
    }

    #[test]
    fn switching_closes_an_open_edit() {
        let mut s = settings();
        press(&mut s, KeyCode::Enter);
        s.open_page(Page::Ui);
        assert!(s.edit.is_none());
    }

    #[test]
    fn down_stays_on_the_page() {
        let mut s = settings();
        for _ in 0..6 {
            press(&mut s, KeyCode::Down);
        }
        assert_eq!((s.page, s.row()), (Page::Worktrees, Some(Row::MergedPullRequests)));
    }
}

#[test]
fn agents_with_modes_are_listed() {
    assert_eq!(settings().listed_kinds(), ["claude", "codex", "gemini", "opencode", "grok", "cursor", "copilot"]);
}

mod folder {
    use super::*;

    #[test]
    fn is_edited_and_saved() {
        let mut s = settings();
        press(&mut s, KeyCode::Enter);
        s.edit.as_mut().expect("editing").input.clear();
        type_text(&mut s, "/srv/w");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).worktrees_dir, "/srv/w");
    }

    #[test]
    fn a_relative_one_is_refused() {
        let mut s = settings();
        press(&mut s, KeyCode::Enter);
        s.edit.as_mut().expect("editing").input = "rel".into();
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert!(s.edit.as_ref().is_some_and(|e| e.error.is_some()));
    }
}

mod fetch {
    use super::*;

    #[test]
    fn is_edited_and_saved() {
        let mut s = settings();
        go_to(&mut s, &Row::Fetch);
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Backspace);
        type_text(&mut s, "15");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).fetch_minutes, 15);
    }

    #[test]
    fn zero_turns_it_off() {
        let mut s = settings();
        go_to(&mut s, &Row::Fetch);
        press(&mut s, KeyCode::Enter);
        s.edit.as_mut().expect("editing").input = "0".into();
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.row_text(&Row::Fetch).value, "off");
    }

    #[test]
    fn a_word_is_refused() {
        let mut s = settings();
        go_to(&mut s, &Row::Fetch);
        press(&mut s, KeyCode::Enter);
        s.edit.as_mut().expect("editing").input = "often".into();
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert!(s.edit.as_ref().is_some_and(|e| e.error.is_some()));
    }
}

mod accounts;

mod messengers;

mod tokens;

mod tabs {
    use super::*;

    #[test]
    fn enter_hides_a_tab() {
        let mut s = settings();
        go_to(&mut s, &Row::Tab("shortcut"));
        let tabs = saved(press(&mut s, KeyCode::Enter)).issue_tabs;
        assert_eq!(tabs, ["all", "github", "gitlab", "linear", "jira", "plane", "asana"]);
    }

    #[test]
    fn a_hidden_tab_comes_back_at_the_end() {
        let mut s = settings();
        s.config.issue_tabs = vec!["github".into()];
        go_to(&mut s, &Row::Tab("all"));
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).issue_tabs, ["github", "all"]);
    }

    #[test]
    fn the_last_one_stays() {
        let mut s = settings();
        s.config.issue_tabs = vec!["github".into()];
        go_to(&mut s, &Row::Tab("github"));
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
    }

    #[test]
    fn shift_up_moves_a_tab_earlier() {
        let mut s = settings();
        go_to(&mut s, &Row::Tab("linear"));
        let config = saved(key(&mut s, KeyCode::Up, KeyModifiers::SHIFT));
        assert_eq!(config.issue_tabs, ["all", "github", "gitlab", "linear", "shortcut", "jira", "plane", "asana"]);
        assert_eq!(s.row(), Some(Row::Tab("linear")));
    }
}

mod panes {
    use super::*;

    #[test]
    fn dimming_inactive_ones_is_a_switch() {
        let mut s = settings();
        go_to(&mut s, &Row::DimPanes);
        assert!(!saved(press(&mut s, KeyCode::Enter)).dim_inactive_panes);
    }
}

mod agents_section {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_off() {
        let mut s = settings();
        let before = s.config.agents_section;
        go_to(&mut s, &Row::AgentsSection);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).agents_section), (false, true));
    }
}

mod working_icon {
    use super::*;

    #[test]
    fn starts_on_the_moon() {
        let mut s = settings();
        s.open_page(Page::Ui);
        let row = s.row_text(&Row::WorkingIcon);
        assert_eq!((row.label.as_str(), row.value.as_str()), ("working icon", "moon"));
    }

    #[test]
    fn the_spinner_is_picked_from_a_list() {
        let mut s = settings();
        go_to(&mut s, &Row::WorkingIcon);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "spin");
        let config = saved(press(&mut s, KeyCode::Enter));
        assert_eq!((config.working_icon.as_str(), config.spinner()), ("spinner", true));
    }

    #[test]
    fn the_list_starts_on_the_current_choice() {
        let mut s = settings();
        s.config.working_icon = "spinner".into();
        go_to(&mut s, &Row::WorkingIcon);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.pick_selected().map(|i| s.pick_choices()[i].value.as_str()), Some("spinner"));
    }
}

mod icons {
    use super::*;

    #[test]
    fn starts_on_auto() {
        let mut s = settings();
        s.open_page(Page::Ui);
        let row = s.row_text(&Row::Icons);
        assert_eq!((row.label.as_str(), row.value.as_str()), ("icons", "auto"));
    }

    #[test]
    fn is_picked_from_a_list() {
        let mut s = settings();
        go_to(&mut s, &Row::Icons);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "off");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).icons, "off");
    }

    #[test]
    fn the_list_starts_on_the_current_choice() {
        let mut s = settings();
        s.config.icons = "on".into();
        go_to(&mut s, &Row::Icons);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.pick_selected().map(|i| s.pick_choices()[i].value.as_str()), Some("on"));
    }

    #[test]
    fn an_unknown_value_shows_as_auto() {
        let mut s = settings();
        s.config.icons = "sometimes".into();
        s.open_page(Page::Ui);
        assert_eq!(s.row_text(&Row::Icons).value, "auto");
    }
}

mod remote {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_off() {
        let mut s = settings();
        let before = s.config.remote;
        go_to(&mut s, &Row::Remote);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).remote), (false, true));
    }

    #[test]
    fn paired_browsers_show_only_when_there_are_some() {
        let mut s = settings();
        s.open_page(Page::Remote);
        let without = s.rows().contains(&Row::Browsers);
        s.browsers = 2;
        assert_eq!((without, s.rows().contains(&Row::Browsers)), (false, true));
    }

    #[test]
    fn enter_on_paired_browsers_asks_to_forget_them_and_keeps_the_row_until_they_are() {
        let mut s = settings();
        s.browsers = 1;
        go_to(&mut s, &Row::Browsers);
        assert_eq!(press(&mut s, KeyCode::Enter), Action::ForgetBrowsers);
        assert!(s.rows().contains(&Row::Browsers) && s.notice.is_none());
    }
}

mod keep_awake {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_off() {
        let mut s = settings();
        let before = s.config.keep_awake;
        go_to(&mut s, &Row::KeepAwake);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).keep_awake), (false, true));
    }
}

mod pull_requests {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_on() {
        let mut s = settings();
        let before = s.config.pull_requests;
        go_to(&mut s, &Row::PullRequests);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).pull_requests), (true, false));
    }

    #[test]
    fn its_note_is_what_the_app_says() {
        let mut s = settings();
        s.pulls = "install gh, GitHub's CLI, to see them".into();

        let text = s.row_text(&Row::PullRequests);

        assert_eq!((text.label.as_str(), text.value.as_str()), ("pull requests", "[x] shown"));
        assert_eq!(text.note, "install gh, GitHub's CLI, to see them");
    }
}

mod merged_pull_requests {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_on() {
        let mut s = settings();
        let before = s.config.merged_pull_requests;
        go_to(&mut s, &Row::MergedPullRequests);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).merged_pull_requests), (true, false));
    }

    #[test]
    fn its_note_is_what_the_app_says() {
        let mut s = settings();
        s.merged = "from 1 GitHub account".into();

        let text = s.row_text(&Row::MergedPullRequests);

        assert_eq!((text.label.as_str(), text.value.as_str()), ("merged PRs in stats", "[x] shown"));
        assert_eq!(text.note, "from 1 GitHub account");
    }
}

mod counts {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_on() {
        let mut s = settings();
        let before = s.config.counts;
        go_to(&mut s, &Row::Counts);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).counts), (true, false));
    }
}

mod icon_bar {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_off() {
        let mut s = settings();
        let before = s.config.icon_bar;
        go_to(&mut s, &Row::IconBar);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).icon_bar), (false, true));
    }

    #[test]
    fn says_it_needs_a_nerd_font() {
        let text = settings().row_text(&Row::IconBar);
        assert_eq!((text.value.as_str(), text.note.contains("Nerd Font")), ("[ ] off", true));
    }
}

mod todo_dot {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_on() {
        let mut s = settings();
        let before = s.config.todo_dot;
        go_to(&mut s, &Row::TodoDot);
        assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).todo_dot), (true, false));
    }
}

mod agent_tabs {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::model(Detail::Model, |c: &Config| c.model)]
    #[case::context(Detail::Context, |c: &Config| c.context)]
    #[case::memory(Detail::Memory, |c: &Config| c.memory)]
    #[case::account(Detail::Account, |c: &Config| c.account_mark)]
    #[case::account_switcher(Detail::AccountSwitcher, |c: &Config| c.account_switcher)]
    #[case::account_picker(Detail::AccountPicker, |c: &Config| c.account_picker)]
    fn each_part_is_a_switch(#[case] detail: Detail, #[case] on: fn(&Config) -> bool) {
        let mut s = settings();
        let before = on(&s.config);
        go_to(&mut s, &Row::Detail(detail));
        assert_eq!(on(&saved(press(&mut s, KeyCode::Enter))), !before);
    }

    #[test]
    fn show_whether_each_part_is_on() {
        let mut s = settings();
        s.config.context = false;
        s.open_page(Page::Ui);
        let texts: Vec<RowText> = s.rows()[9..12].iter().map(|row| s.row_text(row)).collect();
        let rows: Vec<(&str, &str)> = texts.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect();
        assert_eq!(rows, [("model", "[x] shown"), ("context", "[ ] hidden"), ("memory", "[ ] hidden")]);
    }
}

pub(crate) fn ui_rows() -> Vec<Row> {
    vec![
        Row::Sidebar,
        Row::Tabs,
        Row::IconBar,
        Row::Icons,
        Row::AgentsSection,
        Row::WorkingIcon,
        Row::Counts,
        Row::TodoDot,
        Row::DimPanes,
        Row::Detail(Detail::Model),
        Row::Detail(Detail::Context),
        Row::Detail(Detail::Memory),
        Row::Detail(Detail::Account),
        Row::Detail(Detail::AccountSwitcher),
        Row::Detail(Detail::AccountPicker),
        Row::Notifications,
        Row::Sound,
        Row::SoundWaiting,
        Row::SoundDone,
        Row::SoundSubagents,
        Row::Updates,
        Row::StarPrompt,
        Row::Tour,
        Row::TourStart,
        Row::TourReset,
        Row::Prefix,
    ]
}

mod sidebar {
    use super::*;

    #[test]
    fn comes_first_on_the_ui_page() {
        let mut s = settings();
        s.open_page(Page::Ui);
        assert_eq!(s.rows(), ui_rows());
    }

    #[test]
    fn is_picked_from_a_list() {
        let mut s = settings();
        go_to(&mut s, &Row::Sidebar);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "workspaces");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).sidebar, "workspaces_on_top");
    }

    #[test]
    fn the_list_starts_on_the_current_choice() {
        let mut s = settings();
        s.config.sidebar = "projects_on_top".into();
        go_to(&mut s, &Row::Sidebar);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.pick_selected().map(|i| s.pick_choices()[i].value.as_str()), Some("projects_on_top"));
    }

    #[test]
    fn the_tabs_are_picked_from_a_list() {
        let mut s = settings();
        go_to(&mut s, &Row::Tabs);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "top");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).tabs, "top");
    }

    #[test]
    fn the_tabs_start_in_the_sidebar() {
        let mut s = settings();
        s.open_page(Page::Ui);
        let row = s.row_text(&Row::Tabs);
        assert_eq!((row.label.as_str(), row.value.as_str()), ("tabs", "sidebar"));
    }

    #[test]
    fn an_unknown_value_shows_as_the_default() {
        let mut s = settings();
        s.config.sidebar = "sideways".into();
        s.open_page(Page::Ui);
        assert_eq!(s.row_text(&Row::Sidebar).value, "projects_on_top");
    }
}

mod notifications {
    use super::*;

    #[test]
    fn are_picked_from_a_list() {
        let mut s = settings();
        go_to(&mut s, &Row::Notifications);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "off");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).desktop_notifications, "off");
    }

    #[test]
    fn the_list_starts_on_the_current_choice() {
        let mut s = settings();
        s.config.desktop_notifications = "osc9".into();
        go_to(&mut s, &Row::Notifications);
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.pick_selected().map(|i| s.pick_choices()[i].value.as_str()), Some("osc9"));
    }
}

mod sound {
    use super::*;

    #[test]
    fn the_sound_starts_on_and_the_subagents_off() {
        let mut s = settings();
        go_to(&mut s, &Row::Sound);
        assert!(!saved(press(&mut s, KeyCode::Enter)).sound);
        go_to(&mut s, &Row::SoundSubagents);
        assert!(saved(press(&mut s, KeyCode::Enter)).sound_subagents);
    }

    #[test]
    fn an_empty_file_shows_the_built_in_one() {
        let mut s = settings();
        s.config.sound_done = "~/done.wav".into();
        let shown = |s: &Settings, row| s.row_text(row).value;
        assert_eq!(
            (shown(&s, &Row::SoundWaiting), shown(&s, &Row::SoundDone)),
            ("built-in".to_string(), "~/done.wav".to_string())
        );
    }

    #[test]
    fn a_file_is_typed_and_saved_as_it_is() {
        let mut s = settings();
        go_to(&mut s, &Row::SoundWaiting);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "~/ping.wav");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).sound_waiting, "~/ping.wav");
    }

    #[test]
    fn a_file_with_a_control_character_is_refused() {
        let mut s = settings();
        go_to(&mut s, &Row::SoundDone);
        press(&mut s, KeyCode::Enter);
        s.edit.as_mut().expect("editing").input = "a\u{1b}[2Jb.wav".into();
        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert!(s.edit.as_ref().and_then(|edit| edit.error.as_ref()).is_some());
        assert_eq!(s.config.sound_done, "");
    }
}

mod prefix {
    use super::*;

    fn capturing() -> Settings {
        let mut s = settings();
        go_to(&mut s, &Row::Prefix);
        press(&mut s, KeyCode::Enter);
        s
    }

    fn value(s: &Settings) -> String {
        s.row_text(&Row::Prefix).value
    }

    #[test]
    fn is_off_until_one_is_pressed() {
        let mut s = settings();
        s.open_page(Page::Ui);
        assert_eq!(value(&s), "off");
    }

    #[test]
    fn takes_the_next_key_pressed() {
        let mut s = capturing();
        assert_eq!(value(&s), "press a key…");
        assert_eq!(saved(key(&mut s, KeyCode::Char(']'), KeyModifiers::CONTROL)).prefix_key, "ctrl+]");
    }

    #[test]
    fn a_key_that_cannot_be_one_keeps_waiting() {
        let mut s = capturing();
        assert_eq!(press(&mut s, KeyCode::Char('k')), Action::None);
        assert_eq!((s.capturing, s.notice.as_deref()), (true, Some("use ctrl or alt with a key, or a function key")));
    }

    #[test]
    fn a_known_clash_is_named() {
        let mut s = capturing();
        key(&mut s, KeyCode::Char('b'), KeyModifiers::CONTROL);
        let notice = "ctrl+b opens the keys menu; it is also tmux's prefix, so it never reaches a tmux inside a pane";
        assert_eq!(s.notice.as_deref(), Some(notice));
    }

    #[test]
    fn backspace_turns_it_off() {
        let mut s = capturing();
        s.config.prefix_key = "ctrl+]".into();
        assert_eq!(saved(press(&mut s, KeyCode::Backspace)).prefix_key, "");
    }

    #[test]
    fn esc_keeps_the_one_there_was() {
        let mut s = capturing();
        assert_eq!((press(&mut s, KeyCode::Esc), s.capturing), (Action::None, false));
    }
}

mod updates {
    use super::*;

    #[test]
    fn checking_for_them_is_a_switch() {
        let mut s = settings();
        go_to(&mut s, &Row::Updates);
        assert!(!saved(press(&mut s, KeyCode::Enter)).check_updates);
    }
}

mod star_prompt {
    use super::*;

    #[test]
    fn is_a_switch_that_starts_on() {
        let mut s = settings();
        go_to(&mut s, &Row::StarPrompt);
        assert!(!saved(press(&mut s, KeyCode::Enter)).star_prompt);
    }
}

mod agent {
    use super::*;

    #[test]
    fn the_default_is_picked_from_the_known_agents() {
        let mut s = settings();
        go_to(&mut s, &Row::DefaultAgent);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "codex");
        assert_eq!(saved(press(&mut s, KeyCode::Enter)).agent, "codex");
    }

    #[test]
    fn submit_and_trust_are_switches() {
        let mut s = settings();
        go_to(&mut s, &Row::Submit);
        let submit = saved(press(&mut s, KeyCode::Enter)).submit;
        go_to(&mut s, &Row::Trust);
        let trust = saved(press(&mut s, KeyCode::Enter)).accept_trust_prompts;
        assert_eq!((submit, trust), (true, true));
    }

    #[test]
    fn a_mode_sets_the_arguments_of_that_agent() {
        let mut s = settings();
        go_to(&mut s, &Row::Kind("claude".into()));
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "plan");
        let config = saved(press(&mut s, KeyCode::Enter));
        assert_eq!(config.agent_args["claude"], ["--permission-mode", "plan"]);
    }

    #[test]
    fn extra_arguments_are_typed_after_the_mode() {
        let mut s = settings();
        s.config.agent_args.insert("claude".into(), vec!["--permission-mode".into(), "plan".into()]);
        go_to(&mut s, &Row::Kind("claude".into()));
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "extra");
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "--add-dir '../my dir'");
        let config = saved(press(&mut s, KeyCode::Enter));
        assert_eq!(config.agent_args["claude"], ["--permission-mode", "plan", "--add-dir", "../my dir"]);
    }

    #[test]
    fn default_mode_with_no_extras_forgets_the_agent_arguments() {
        let mut s = settings();
        s.config.agent_args.insert("codex".into(), vec!["--sandbox".into(), "read-only".into()]);
        go_to(&mut s, &Row::Kind("codex".into()));
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "default");
        assert!(!saved(press(&mut s, KeyCode::Enter)).agent_args.contains_key("codex"));
    }

    #[test]
    fn another_agent_is_added_and_asks_for_its_arguments() {
        let mut s = settings();
        go_to(&mut s, &Row::AddAgent);
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "aider");
        press(&mut s, KeyCode::Enter);
        type_text(&mut s, "--model x");
        let config = saved(press(&mut s, KeyCode::Enter));
        assert_eq!(config.agent_args["aider"], ["--model", "x"]);
    }

    #[test]
    fn a_dangerous_mode_is_marked() {
        let mut s = settings();
        s.config.agent_args.insert("claude".into(), vec!["--dangerously-skip-permissions".into()]);
        s.open_page(Page::Agents);
        let claude = s.row_text(&Row::Kind("claude".into()));
        assert_eq!((claude.value.as_str(), claude.dangerous), ("skip permissions (dangerous)", true));
    }
}

#[test]
fn esc_closes_a_pick_before_the_settings() {
    let mut s = settings();
    go_to(&mut s, &Row::DefaultAgent);
    press(&mut s, KeyCode::Enter);
    let first = press(&mut s, KeyCode::Esc);
    assert_eq!((first, s.pick.is_none(), press(&mut s, KeyCode::Esc)), (Action::None, true, Action::Close));
}

#[test]
fn keys_wait_while_a_token_is_checked() {
    let mut s = settings();
    s.checking.push(Source::Shortcut);
    assert_eq!(press(&mut s, KeyCode::Esc), Action::None);
}
mod requests;
