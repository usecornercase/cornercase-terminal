use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::style::Color;
use ratatui::text::Line;

use super::view::detail_lines;
use super::*;
use crate::issues::{Comment, issue};

const ROOM: Room = Room { list_rows: 22, detail_rows: 26, detail_width: 106 };

fn browser() -> Browser {
    Browser {
        project: 1,
        project_name: "shop".into(),
        github: true,
        worktrees: true,
        tabs: tabs(&[]),
        tab: 0,
        closed: false,
        people: People::default(),
        search: Search::default(),
        lists: HashMap::new(),
        connections: HashMap::new(),
        forms: HashMap::new(),
        jira_site: String::new(),
        jira_email: String::new(),
        plane_workspace: String::new(),
        gitlab_url: String::new(),
        screen: Screen::List,
        starting: false,
        error: None,
        notice: None,
        secrets_path: "~/.config/cornercase/secrets.json".into(),
        agents: agents(Some("claude")),
        picker: None,
        places: vec![shop()],
        here: 0,
        place: None,
        filtering: None,
        members: HashMap::new(),
    }
}

fn shop() -> Place {
    Place { project: 1, workspace: None, label: "shop".into(), worktree: true }
}

fn notes() -> Place {
    Place { project: 2, workspace: Some(20), label: "notes › default".into(), worktree: false }
}

fn agents(default: Option<&str>) -> Agents {
    let start =
        |command: &str, mode: Option<&str>| AgentStart { command: command.into(), mode: mode.map(String::from) };
    Agents {
        kinds: vec!["claude".into(), "codex".into()],
        default: default.map(String::from),
        running: None,
        chosen: None,
        starts: HashMap::from([
            (
                "claude".to_string(),
                start("claude --dangerously-skip-permissions", Some("skip permissions (dangerous)")),
            ),
            ("codex".to_string(), start("codex", None)),
        ]),
        prompt: "{url}".into(),
        submit: false,
    }
}

pub(crate) fn with_lists() -> Browser {
    let mut b = browser();
    b.connected(Source::Shortcut, Connection { from_env: false, account: None });
    let old = Issue { updated_at: "2026-01-01T00:00:00Z".into(), ..issue(Source::Github, 1, "old bug") };
    let new = Issue { updated_at: "2026-09-01T00:00:00Z".into(), ..issue(Source::Shortcut, 2, "new story") };
    b.loaded(Source::Github, &Query::default(), Ok(vec![old]));
    b.loaded(Source::Shortcut, &Query::default(), Ok(vec![new]));
    b
}

pub(crate) fn press(b: &mut Browser, code: KeyCode) -> Action {
    b.key(
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        },
        ROOM,
    )
}

fn type_text(b: &mut Browser, text: &str) {
    for c in text.chars() {
        press(b, KeyCode::Char(c));
    }
}

fn keys(b: &Browser) -> Vec<String> {
    b.shown().iter().map(|i| i.key.clone()).collect()
}

fn click(b: &mut Browser, hit: IssuesHit) -> Action {
    b.mouse(MouseEventKind::Down(MouseButton::Left), Some(hit), ROOM)
}

fn button(b: &Browser, label: &str) -> IssuesHit {
    let labels = b.labels();
    let i = labels.iter().position(|l| *l == label).expect("the button is there");
    IssuesHit::Button(i)
}

mod tabs {
    use super::*;

    #[test]
    fn are_all_seven_by_default() {
        assert_eq!(
            tabs(&[]),
            [
                Tab::All,
                Tab::One(Source::Github),
                Tab::One(Source::Gitlab),
                Tab::One(Source::Shortcut),
                Tab::One(Source::Linear),
                Tab::One(Source::Jira),
                Tab::One(Source::Plane)
            ]
        );
    }

    #[test]
    fn follow_the_config_and_skip_unknown_names() {
        let names = ["linear", "nope", "GitHub", "linear"].map(String::from);
        assert_eq!(tabs(&names), [Tab::One(Source::Linear), Tab::One(Source::Github)]);
    }

    #[test]
    fn all_lists_every_source_newest_first() {
        assert_eq!(keys(&with_lists()), ["sc-2", "#1"]);
    }

    #[test]
    fn a_source_tab_lists_only_its_issues() {
        let mut b = with_lists();
        b.tab_to(Tab::One(Source::Github));
        assert_eq!(keys(&b), ["#1"]);
    }

    #[test]
    fn tab_moves_to_the_next_one_and_asks_for_its_issues() {
        let mut b = browser();
        assert_eq!(
            (press(&mut b, KeyCode::Tab), b.current()),
            (Action::Load(vec![Source::Github]), Tab::One(Source::Github))
        );
    }

    #[test]
    fn opening_all_loads_github_and_the_connected_trackers() {
        let mut b = browser();
        b.connected(Source::Linear, Connection { from_env: true, account: None });
        assert_eq!(b.needs_load(), Action::Load(vec![Source::Github, Source::Linear]));
    }

    #[test]
    fn outside_git_github_is_not_asked() {
        let b = Browser { github: false, ..browser() };
        assert_eq!(b.needs_load(), Action::None);
    }

    #[test]
    fn a_click_on_a_tab_switches_to_it() {
        let mut b = browser();
        click(&mut b, IssuesHit::Tab(3));
        assert_eq!(b.current(), Tab::One(Source::Shortcut));
    }

    #[test]
    fn gitlab_is_asked_once_connected() {
        let mut b = browser();
        b.connected(Source::Gitlab, Connection { from_env: true, account: None });
        assert_eq!(b.needs_load(), Action::Load(vec![Source::Github, Source::Gitlab]));
    }

    #[test]
    fn outside_git_gitlab_is_not_asked_either() {
        let mut b = Browser { github: false, ..browser() };
        b.connected(Source::Gitlab, Connection { from_env: true, account: None });
        b.connected(Source::Linear, Connection { from_env: true, account: None });
        assert_eq!(b.needs_load(), Action::Load(vec![Source::Linear]));
    }
}

mod filter {
    use super::*;

    #[test]
    fn typing_filters_the_list() {
        let mut b = with_lists();
        type_text(&mut b, "bug");
        assert_eq!(keys(&b), ["#1"]);
    }

    #[test]
    fn closed_reloads_everything_with_the_new_query() {
        let mut b = with_lists();
        let toggle = IssuesHit::Toggle(0);

        let action = click(&mut b, toggle);

        assert_eq!(
            (action, b.closed, keys(&b)),
            (Action::Load(vec![Source::Github, Source::Shortcut]), true, Vec::<String>::new())
        );
    }

    #[test]
    fn results_for_an_old_query_are_dropped() {
        let mut b = browser();
        b.people.github[0] = Who::Me;
        b.loaded(Source::Github, &Query::default(), Ok(vec![issue(Source::Github, 1, "x")]));
        assert_eq!(b.shown(), Vec::<&Issue>::new());
    }
}

mod detail {
    use super::*;

    #[test]
    fn enter_reads_the_selected_issue() {
        let mut b = with_lists();
        let action = press(&mut b, KeyCode::Enter);
        assert!(
            matches!((&action, &b.screen), (Action::Read(i), Screen::Detail { .. }) if i.key == "sc-2"),
            "{action:?}"
        );
    }

    #[test]
    fn a_click_on_an_issue_reads_it() {
        let mut b = with_lists();
        let action = click(&mut b, IssuesHit::Item(1));
        assert!(matches!(action, Action::Read(i) if i.key == "#1"));
    }

    #[test]
    fn enter_in_the_issue_starts_it() {
        let mut b = with_lists();
        press(&mut b, KeyCode::Enter);
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(i, _, _) if i.key == "sc-2"));
    }

    #[test]
    fn esc_goes_back_to_the_list() {
        let mut b = with_lists();
        press(&mut b, KeyCode::Enter);
        assert_eq!((press(&mut b, KeyCode::Esc), matches!(b.screen, Screen::List)), (Action::None, true));
    }

    #[test]
    fn the_answer_for_another_issue_is_ignored() {
        let mut b = with_lists();
        press(&mut b, KeyCode::Enter);
        b.read_done(Source::Github, "#1", Ok(Detail::default()));
        assert!(matches!(b.screen, Screen::Detail { detail: None, .. }));
    }

    #[test]
    fn scrolling_stops_at_the_end() {
        let mut b = with_lists();
        press(&mut b, KeyCode::Enter);
        let body = (1..200).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n\n");
        b.read_done(Source::Shortcut, "sc-2", Ok(Detail { body, ..Detail::default() }));
        press(&mut b, KeyCode::End);
        press(&mut b, KeyCode::Down);
        let Screen::Detail { scroll, .. } = b.screen else { panic!("not in the detail") };
        assert_eq!(scroll, b.detail_lines(ROOM).len() - ROOM.detail_rows);
    }

    #[test]
    fn shows_the_title_meta_body_and_comments() {
        let detail = Detail {
            info: vec!["open".into(), "bug".into()],
            body: "It **crashes**.".into(),
            comments: vec![Comment { author: "bo".into(), created_at: String::new(), body: "Same".into() }],
        };
        let issue = issue(Source::Github, 7, "Login fails");
        let text: Vec<String> = detail_lines(&issue, Some(&detail), 30, 0, false, Vec::new())
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(
            text,
            [
                "#7 Login fails",
                "open · bug",
                "https://github.com/acme/shop/i",
                "ssues/7",
                "",
                "It crashes.",
                "",
                "── @bo ───────────────────────",
                "Same"
            ]
        );
    }
}

mod tokens {
    use super::*;

    fn on_shortcut() -> Browser {
        let mut b = browser();
        b.tab_to(Tab::One(Source::Shortcut));
        b
    }

    #[test]
    fn a_tracker_without_a_token_asks_for_one() {
        let b = on_shortcut();
        assert_eq!((b.token_form(), b.labels()), (Some(Source::Shortcut), vec!["connect", "cancel"]));
    }

    #[test]
    fn enter_checks_the_typed_token() {
        let mut b = on_shortcut();
        type_text(&mut b, "t0k");
        assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Shortcut, Secret("t0k".into())));
    }

    #[test]
    fn the_token_is_never_shown() {
        let mut b = on_shortcut();
        type_text(&mut b, "t0k");
        assert_eq!(b.token_field(Source::Shortcut).1, "•••");
    }

    #[test]
    fn an_empty_token_is_not_sent() {
        let mut b = on_shortcut();
        assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
    }

    #[test]
    fn once_connected_the_tab_lists_and_offers_to_disconnect() {
        let mut b = on_shortcut();
        b.connected(Source::Shortcut, Connection { from_env: false, account: None });
        assert_eq!(
            (b.needs_load(), b.labels()),
            (Action::Load(vec![Source::Shortcut]), vec!["start", "refresh", "disconnect", "cancel"])
        );
    }

    #[test]
    fn a_token_from_the_environment_cannot_be_disconnected() {
        let mut b = on_shortcut();
        b.connected(Source::Shortcut, Connection { from_env: true, account: None });
        assert_eq!(b.labels(), ["start", "refresh", "cancel"]);
    }

    fn field(b: &Browser) -> (&'static str, String) {
        b.token_field(b.token_form().expect("the token form"))
    }

    mod jira {
        use super::*;

        fn on_jira() -> Browser {
            let mut b = browser();
            b.jira_site = "acme.atlassian.net".into();
            b.jira_email = "ana@acme.dev".into();
            b.tab_to(Tab::One(Source::Jira));
            b
        }

        fn to_the_token(b: &mut Browser) -> Action {
            press(b, KeyCode::Enter);
            press(b, KeyCode::Enter)
        }

        #[test]
        fn asks_the_site_first_with_the_saved_one_typed() {
            let b = on_jira();
            assert_eq!((field(&b), b.labels()), (("site", "acme.atlassian.net".into()), vec!["next", "cancel"]));
        }

        #[test]
        fn then_the_email_and_saves_both() {
            let mut b = on_jira();
            press(&mut b, KeyCode::Enter);
            assert_eq!((field(&b), b.labels()), (("email", "ana@acme.dev".into()), vec!["next", "back", "cancel"]));
            let saved = press(&mut b, KeyCode::Enter);
            assert_eq!(saved, Action::SaveJira { site: "acme.atlassian.net".into(), email: "ana@acme.dev".into() });
        }

        #[test]
        fn a_site_typed_as_a_name_gets_its_domain() {
            let mut b = on_jira();
            b.jira_site.clear();
            type_text(&mut b, "shop");
            press(&mut b, KeyCode::Enter);
            assert_eq!(b.jira_site, "shop.atlassian.net");
        }

        #[test]
        fn a_bad_email_stays_with_its_error() {
            let mut b = on_jira();
            b.jira_email = "ana".into();
            let action = to_the_token(&mut b);
            let error = b.forms.get(&Source::Jira).and_then(|f| f.error.clone());
            assert_eq!(
                (action, field(&b).0, error.as_deref()),
                (Action::None, "email", Some("type the email of your Atlassian account"))
            );
        }

        #[test]
        fn the_token_comes_last_and_is_hidden() {
            let mut b = on_jira();
            to_the_token(&mut b);
            type_text(&mut b, "t0k");
            assert_eq!((field(&b), b.labels()), (("API token", "•••".into()), vec!["connect", "back", "cancel"]));
            assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Jira, Secret("t0k".into())));
        }

        #[test]
        fn back_returns_to_the_email() {
            let mut b = on_jira();
            to_the_token(&mut b);
            let pos = button(&b, "back");
            click(&mut b, pos);
            assert_eq!(field(&b), ("email", "ana@acme.dev".into()));
        }
    }

    mod plane {
        use super::*;

        fn on_plane() -> Browser {
            let mut b = browser();
            b.plane_workspace = "acme".into();
            b.tab_to(Tab::One(Source::Plane));
            b
        }

        #[test]
        fn asks_the_workspace_first_with_the_saved_one_typed() {
            let b = on_plane();
            assert_eq!((field(&b), b.labels()), (("workspace", "acme".into()), vec!["next", "cancel"]));
        }

        #[test]
        fn saves_the_workspace_and_then_asks_for_the_key() {
            let mut b = on_plane();
            assert_eq!(press(&mut b, KeyCode::Enter), Action::SavePlane { workspace: "acme".into() });
            assert_eq!((field(&b), b.labels()), (("API key", String::new()), vec!["connect", "back", "cancel"]));
        }

        #[test]
        fn a_workspace_typed_in_capitals_is_saved_as_a_slug() {
            let mut b = on_plane();
            b.plane_workspace.clear();
            type_text(&mut b, " Acme-Team ");
            press(&mut b, KeyCode::Enter);
            assert_eq!(b.plane_workspace, "acme-team");
        }

        #[test]
        fn a_bad_workspace_stays_with_its_error() {
            let mut b = on_plane();
            b.plane_workspace = "acme corp".into();
            let action = press(&mut b, KeyCode::Enter);
            let error = b.forms.get(&Source::Plane).and_then(|f| f.error.clone());
            let expected = "a workspace is lowercase letters, numbers and dashes, such as acme";
            assert_eq!((action, field(&b).0, error.as_deref()), (Action::None, "workspace", Some(expected)));
        }

        #[test]
        fn the_key_is_hidden_and_checked() {
            let mut b = on_plane();
            press(&mut b, KeyCode::Enter);
            type_text(&mut b, "k3y");
            assert_eq!(field(&b), ("API key", "•••".into()));
            assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Plane, Secret("k3y".into())));
        }

        #[test]
        fn back_returns_to_the_workspace() {
            let mut b = on_plane();
            press(&mut b, KeyCode::Enter);
            let pos = button(&b, "back");
            click(&mut b, pos);
            assert_eq!(field(&b), ("workspace", "acme".into()));
        }
    }

    mod gitlab {
        use super::*;

        fn on_gitlab() -> Browser {
            let mut b = browser();
            b.tab_to(Tab::One(Source::Gitlab));
            b
        }

        #[test]
        fn asks_the_server_first_with_the_saved_one_typed() {
            let mut b = on_gitlab();
            b.gitlab_url = "https://gitlab.acme.dev".into();
            assert_eq!((field(&b), b.labels()), (("server", "https://gitlab.acme.dev".into()), vec!["next", "cancel"]));
        }

        #[test]
        fn an_empty_server_is_saved_and_then_the_token_is_asked() {
            let mut b = on_gitlab();
            assert_eq!(press(&mut b, KeyCode::Enter), Action::SaveGitlab { url: String::new() });
            assert_eq!((field(&b), b.labels()), (("access token", String::new()), vec!["connect", "back", "cancel"]));
        }

        #[test]
        fn a_typed_server_is_saved_without_its_last_slash() {
            let mut b = on_gitlab();
            type_text(&mut b, "https://gitlab.acme.dev/");
            assert_eq!(press(&mut b, KeyCode::Enter), Action::SaveGitlab { url: "https://gitlab.acme.dev".into() });
            assert_eq!(b.gitlab_url, "https://gitlab.acme.dev");
        }

        #[test]
        fn a_bad_server_stays_with_its_error() {
            let mut b = on_gitlab();
            type_text(&mut b, "gitlab.acme.dev");
            let action = press(&mut b, KeyCode::Enter);
            let error = b.forms.get(&Source::Gitlab).and_then(|f| f.error.clone());
            let expected = "a URL starts with https://, such as https://gitlab.acme.dev";
            assert_eq!((action, field(&b).0, error.as_deref()), (Action::None, "server", Some(expected)));
        }

        #[test]
        fn the_token_is_hidden_and_checked() {
            let mut b = on_gitlab();
            press(&mut b, KeyCode::Enter);
            type_text(&mut b, "glpat");
            assert_eq!(field(&b), ("access token", "•••••".into()));
            assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Gitlab, Secret("glpat".into())));
        }

        #[test]
        fn back_returns_to_the_server() {
            let mut b = on_gitlab();
            b.gitlab_url = "https://gitlab.acme.dev".into();
            press(&mut b, KeyCode::Enter);
            let pos = button(&b, "back");
            click(&mut b, pos);
            assert_eq!(field(&b), ("server", "https://gitlab.acme.dev".into()));
        }
    }

    #[test]
    fn disconnect_asks_to_forget_the_token() {
        let mut b = on_shortcut();
        b.connected(Source::Shortcut, Connection { from_env: false, account: None });
        let pos = button(&b, "disconnect");
        assert_eq!(click(&mut b, pos), Action::Disconnect(Source::Shortcut));
    }
}

pub(crate) mod buttons {
    use super::*;

    pub(crate) fn reading() -> Browser {
        let mut b = with_lists();
        press(&mut b, KeyCode::Enter);
        let body = "# Title\n\n    indented **bold**".to_string();
        b.read_done(Source::Shortcut, "sc-2", Ok(Detail { body, ..Detail::default() }));
        b
    }

    fn body_text(b: &Browser) -> Vec<String> {
        let lines: Vec<String> =
            b.detail_lines(ROOM).iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect()).collect();
        let prompt = lines.iter().position(|l| l.starts_with("prompt")).expect("the prompt row");
        lines[prompt + 2..].to_vec()
    }

    #[test]
    fn the_issue_offers_raw_copy_and_back_after_the_agent() {
        assert_eq!(reading().labels(), ["start", "agent…", "raw", "copy url", "back"]);
    }

    #[test]
    fn the_list_offers_refresh() {
        assert_eq!(with_lists().labels(), ["start", "refresh", "cancel"]);
    }

    #[test]
    fn refresh_reloads_what_the_tab_shows() {
        let mut b = with_lists();
        let pos = button(&b, "refresh");
        assert_eq!(click(&mut b, pos), Action::Load(vec![Source::Github, Source::Shortcut]));
    }

    #[test]
    fn raw_shows_the_markdown_as_written() {
        let mut b = reading();
        let pos = button(&b, "raw");
        click(&mut b, pos);
        assert_eq!(
            (b.labels().contains(&"rendered"), body_text(&b)),
            (true, vec!["# Title".to_string(), String::new(), "    indented **bold**".into()])
        );
    }

    #[test]
    fn rendered_goes_back_to_the_formatted_text() {
        let mut b = reading();
        let raw = button(&b, "raw");
        click(&mut b, raw);
        let rendered = button(&b, "rendered");
        click(&mut b, rendered);
        assert_eq!(body_text(&b)[0], "Title");
    }

    #[test]
    fn copy_asks_to_copy_the_url() {
        let mut b = reading();
        let pos = button(&b, "copy url");
        assert_eq!(click(&mut b, pos), Action::Copy("https://github.com/acme/shop/issues/2".into()));
    }

    #[test]
    fn tabs_round_trip_through_their_id() {
        let tabs = [Tab::All, Tab::One(Source::Linear)];
        assert_eq!(tabs.map(|t| Tab::from_id(t.id())), tabs.map(Some));
    }
}

mod agent {
    use super::*;

    fn reading(default: Option<&str>) -> Browser {
        let mut b = Browser { agents: agents(default), ..with_lists() };
        press(&mut b, KeyCode::Enter);
        b
    }

    fn line_with(b: &Browser, label: &str) -> Line<'static> {
        b.detail_lines(ROOM)
            .into_iter()
            .find(|l| l.spans.first().is_some_and(|s| s.content.trim() == label))
            .expect("the row is there")
    }

    #[test]
    fn start_uses_the_default_agent() {
        let mut b = reading(Some("claude"));
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(_, kind, _) if kind == "claude"));
    }

    #[test]
    fn the_agent_running_in_the_tab_is_used_when_the_default_is_auto() {
        let mut b = reading(None);
        b.agents.running = Some("codex".into());
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(_, kind, _) if kind == "codex"));
    }

    #[test]
    fn with_nothing_decided_start_asks_which_agent() {
        let mut b = reading(None);
        let action = press(&mut b, KeyCode::Enter);
        assert_eq!((action, b.picker.as_ref().map(|p| p.pick)), (Action::None, Some(Pick::Agent)));
    }

    #[test]
    fn the_picked_agent_takes_the_issue() {
        let mut b = reading(None);
        press(&mut b, KeyCode::Enter);
        type_text(&mut b, "cod");
        press(&mut b, KeyCode::Enter);
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(_, kind, _) if kind == "codex"));
    }

    #[test]
    fn another_agent_can_become_the_default() {
        let mut b = reading(Some("claude"));
        b.agents.chosen = Some("codex".into());
        let pos = button(&b, "make default");
        assert_eq!(click(&mut b, pos), Action::SetDefaultAgent("codex".into()));
    }

    #[test]
    fn the_issue_shows_how_the_agent_starts() {
        let b = reading(Some("claude"));
        let text: String = line_with(&b, "agent").spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(
            text,
            "agent   claude · skip permissions (dangerous)  claude --dangerously-skip-permissions  (default)"
        );
    }

    #[test]
    fn a_dangerous_mode_is_red() {
        let b = reading(Some("claude"));
        let line = line_with(&b, "agent");
        let mode = line.spans.iter().find(|s| s.content.contains("dangerous")).expect("the mode");
        assert_eq!(mode.style.fg, Some(Color::Red));
    }

    #[test]
    fn the_issue_shows_the_prompt_and_how_it_is_sent() {
        let b = reading(Some("claude"));
        let text: String = line_with(&b, "prompt").spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "prompt  https://github.com/acme/shop/issues/2  (typed, you press enter)");
    }
}

mod place {
    use super::*;

    fn two_places() -> Browser {
        let mut b = Browser { places: vec![shop(), notes()], ..with_lists() };
        press(&mut b, KeyCode::Enter);
        b
    }

    #[test]
    fn a_story_asks_where_first_with_the_current_project_preselected() {
        let mut b = two_places();
        let first = press(&mut b, KeyCode::Enter);
        let started = press(&mut b, KeyCode::Enter);
        assert_eq!(first, Action::None);
        assert!(matches!(&started, Action::Start(_, _, Some(place)) if *place == shop()), "{started:?}");
    }

    #[test]
    fn another_place_can_be_picked() {
        let mut b = two_places();
        press(&mut b, KeyCode::Enter);
        type_text(&mut b, "notes");
        let started = press(&mut b, KeyCode::Enter);
        assert!(matches!(&started, Action::Start(_, _, Some(place)) if *place == notes()), "{started:?}");
    }

    #[test]
    fn the_issue_shows_where_it_starts() {
        let mut b = two_places();
        b.place = Some(1);
        let start: String = b
            .detail_lines(ROOM)
            .into_iter()
            .find(|l| l.spans.first().is_some_and(|s| s.content.trim() == "start"))
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .expect("the start row");
        assert_eq!(start, "start   in a new tab in notes › default");
    }

    #[test]
    fn where_is_offered_for_stories_only() {
        let mut b = two_places();
        let story = b.labels();
        press(&mut b, KeyCode::Esc);
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Enter);
        assert_eq!((story.contains(&"where…"), b.labels().contains(&"where…")), (true, false));
    }

    #[test]
    fn a_gitlab_issue_starts_in_its_repository_like_a_github_one() {
        let mut b = Browser { places: vec![shop(), notes()], ..browser() };
        b.connected(Source::Gitlab, Connection { from_env: true, account: None });
        b.tab_to(Tab::One(Source::Gitlab));
        b.loaded(Source::Gitlab, &Query::default(), Ok(vec![issue(Source::Gitlab, 5, "Checkout fails")]));
        press(&mut b, KeyCode::Enter);
        assert!(!b.labels().contains(&"where…"), "{:?}", b.labels());
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(i, _, None) if i.key == "#5"));
    }

    #[test]
    fn a_github_issue_starts_in_its_repository() {
        let mut b = two_places();
        press(&mut b, KeyCode::Esc);
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Enter);
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(i, _, None) if i.key == "#1"));
    }
}

pub(crate) mod people {
    use super::*;

    fn toggle() -> IssuesHit {
        IssuesHit::Toggle(1)
    }

    pub(crate) fn filtering() -> Browser {
        let mut b = with_lists();
        click(&mut b, toggle());
        b
    }

    #[test]
    fn the_toggle_opens_the_filter_and_asks_for_the_people() {
        let mut b = with_lists();
        let action = click(&mut b, toggle());
        assert_eq!((action, b.filtering), (Action::LoadPeople(vec![Source::Github, Source::Shortcut]), Some(0)));
    }

    #[test]
    fn me_filters_that_source_and_reloads_it_when_done() {
        let mut b = filtering();
        press(&mut b, KeyCode::Enter);
        type_text(&mut b, "me");
        press(&mut b, KeyCode::Enter);
        let done = press(&mut b, KeyCode::Esc);
        assert_eq!((b.people.github[0].clone(), done), (Who::Me, Action::Load(vec![Source::Github])));
    }

    #[test]
    fn members_can_be_picked_by_name() {
        let mut b = filtering();
        b.people_loaded(
            Source::Shortcut,
            Ok(vec![Person { handle: "bo".into(), name: "Bo Diddley".into(), id: None }]),
        );
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Enter);
        type_text(&mut b, "diddley");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.people.shortcut[1], Who::Person("bo".into()));
    }

    #[test]
    fn jira_people_are_picked_by_account_even_with_the_same_name() {
        let mut b = browser();
        b.tab_to(Tab::One(Source::Jira));
        b.connected(Source::Jira, Connection { from_env: false, account: None });
        b.open_people();
        let twin = |id: &str, email: &str| Person { handle: "Ana".into(), name: email.into(), id: Some(id.into()) };
        b.people_loaded(Source::Jira, Ok(vec![twin("a1", "ana@one.dev"), twin("a2", "ana@two.dev")]));
        press(&mut b, KeyCode::Enter);
        type_text(&mut b, "two");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.people.jira[0], Who::User { id: "a2".into(), name: "Ana".into() });
        assert_eq!(b.people.describe(Source::Jira), "assignee @Ana");
    }

    #[test]
    fn the_toggle_says_who_filters() {
        let mut b = with_lists();
        b.people.shortcut[0] = Who::Me;
        assert_eq!(b.toggles(), (vec!["closed".to_string(), "people: Shortcut owner me".into()], vec![false, true]));
    }

    #[test]
    fn clear_goes_back_to_anyone() {
        let mut b = filtering();
        b.people.github = [Who::Me, Who::Person("x".into())];
        let pos = button(&b, "clear");
        click(&mut b, pos);
        assert_eq!(b.people, People::default());
    }

    #[test]
    fn the_query_of_a_source_carries_its_people() {
        let mut b = with_lists();
        b.people.linear = [Who::Anyone, Who::Me];
        assert_eq!(b.query(Source::Linear), Query { closed: false, people: [Who::Anyone, Who::Me] });
    }
}

#[test]
fn nothing_reacts_while_starting() {
    let mut b = with_lists();
    b.starting = true;
    assert_eq!(press(&mut b, KeyCode::Esc), Action::None);
}

#[test]
fn the_start_button_starts_the_selected_issue() {
    let mut b = with_lists();
    let pos = button(&b, "start");
    assert!(matches!(click(&mut b, pos), Action::Start(i, _, _) if i.key == "sc-2"));
}
