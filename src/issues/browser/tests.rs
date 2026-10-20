use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::style::Color;
use ratatui::text::Line;

use super::view::{Shown, detail_lines};
use super::*;
use crate::issues::{Comment, issue};

const ROOM: Room = Room { list_rows: 22, detail_rows: 26, detail_width: 106, buttons: 110 };

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
        scheduling: false,
        error: None,
        notice: None,
        secrets_path: "~/.config/cornercase/secrets.json".into(),
        agents: agents(Some("claude")),
        accounts: None,
        picker: None,
        naming: None,
        places: vec![shop()],
        here: 0,
        place: None,
        filtering: None,
        members: HashMap::new(),
        queued: Vec::new(),
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
        branch: String::new(),
        user: String::new(),
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
    let labels = b.labels(ROOM.buttons);
    let i = labels.iter().position(|l| *l == label).expect("the button is there");
    IssuesHit::Button(i)
}

mod tabs {
    use super::*;

    #[test]
    fn are_all_eight_and_the_queue_by_default() {
        assert_eq!(
            tabs(&[]),
            [
                Tab::All,
                Tab::One(Source::Github),
                Tab::One(Source::Gitlab),
                Tab::One(Source::Shortcut),
                Tab::One(Source::Linear),
                Tab::One(Source::Jira),
                Tab::One(Source::Plane),
                Tab::One(Source::Asana),
                Tab::Queue
            ]
        );
    }

    #[test]
    fn follow_the_config_and_skip_unknown_names() {
        let names = ["linear", "nope", "GitHub", "linear"].map(String::from);
        assert_eq!(tabs(&names), [Tab::One(Source::Linear), Tab::One(Source::Github), Tab::Queue]);
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
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Name(i, _, _) if i.key == "sc-2"));
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
        let text: Vec<String> =
            detail_lines(Shown { issue: &issue, detail: Some(&detail), width: 30, now: 0, raw: false }, Vec::new())
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

mod tokens;

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
        assert_eq!(
            reading().labels(ROOM.buttons),
            ["start", "schedule", "start a flow", "agent…", "raw", "copy url", "back"]
        );
    }

    #[test]
    fn a_narrow_row_drops_schedule_so_start_stays() {
        let b = reading();
        let all = ["start", "schedule", "start a flow", "agent…", "raw", "copy url", "back"];
        let width = all.iter().map(|label| label.chars().count() + 3).sum::<usize>() - 1;
        assert_eq!(b.labels(width), all);
        assert_eq!(b.labels(width - 1), ["start", "start a flow", "agent…", "raw", "copy url", "back"]);
    }

    #[test]
    fn enter_starts_the_issue_after_schedule_was_pressed() {
        let mut b = reading();
        let schedule = button(&b, "schedule");
        click(&mut b, schedule);
        if b.picker.is_some() {
            press(&mut b, KeyCode::Esc);
        }
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Name(..)));
    }

    #[test]
    fn the_list_offers_a_flow_beside_start_and_refresh() {
        assert_eq!(with_lists().labels(ROOM.buttons), ["start", "schedule", "start a flow", "refresh", "cancel"]);
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
            (b.labels(ROOM.buttons).contains(&"rendered"), body_text(&b)),
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
        let tabs = [Tab::All, Tab::One(Source::Linear), Tab::Queue];
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
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Name(_, kind, _) if kind == "claude"));
    }

    #[test]
    fn the_agent_running_in_the_tab_is_used_when_the_default_is_auto() {
        let mut b = reading(None);
        b.agents.running = Some("codex".into());
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Name(_, kind, _) if kind == "codex"));
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
        assert!(matches!(press(&mut b, KeyCode::Enter), Action::Name(_, kind, _) if kind == "codex"));
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

mod account;
mod list_flow;
mod naming;
mod place;

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
    assert!(matches!(click(&mut b, pos), Action::Name(i, _, _) if i.key == "sc-2"));
}

mod queue_tab {
    use super::*;
    use crate::issues::browser::{Queued, Tab};
    use crate::queue::State;

    fn queued(id: u64, label: &str, state: State) -> Queued {
        Queued { id, when: "2026-10-15 09:00".into(), label: label.into(), meta: "claude · shop".into(), state }
    }

    fn on_queue() -> Browser {
        let mut b = browser();
        b.tabs = tabs(&[]);
        b.tab_to(Tab::Queue);
        b.queued = vec![
            queued(1, "Fix the login", State::Pending),
            queued(2, "Update the docs", State::Started { pane: 7, server: "s".into() }),
            queued(3, "Fix logout", State::Starting),
        ];
        b
    }

    #[test]
    fn the_queue_is_the_last_tab_and_loads_nothing() {
        let b = on_queue();
        assert_eq!(b.tab_names().last(), Some(&"Queue"));
        assert_eq!((b.needs_load(), b.toggles()), (Action::None, (Vec::new(), Vec::new())));
    }

    #[test]
    fn the_buttons_follow_the_selected_entry() {
        let mut b = on_queue();
        assert_eq!(b.labels(ROOM.buttons), ["new", "start now", "remove", "clear started", "cancel"]);
        press(&mut b, KeyCode::Down);
        assert_eq!(b.labels(ROOM.buttons), ["new", "remove", "clear started", "cancel"]);
        press(&mut b, KeyCode::Down);
        assert_eq!(b.labels(ROOM.buttons), ["new", "clear started", "cancel"]);
    }

    #[test]
    fn enter_and_a_click_open_the_entry_and_the_buttons_say_what_to_do() {
        let mut b = on_queue();
        assert_eq!(press(&mut b, KeyCode::Enter), Action::QueueOpen(1));
        assert_eq!(click(&mut b, IssuesHit::Item(1)), Action::QueueOpen(2));
        for (label, action) in
            [("remove", Action::QueueRemove(2)), ("new", Action::QueueNew), ("clear started", Action::QueueClear)]
        {
            let hit = button(&b, label);
            assert_eq!(click(&mut b, hit), action);
        }
        press(&mut b, KeyCode::Up);
        let hit = button(&b, "start now");
        assert_eq!(click(&mut b, hit), Action::QueueRun(1));
    }

    #[test]
    fn typing_filters_the_entries() {
        let mut b = on_queue();
        for c in "logout".chars() {
            press(&mut b, KeyCode::Char(c));
        }
        assert_eq!(b.shown_queue().iter().map(|q| q.id).collect::<Vec<_>>(), [3]);
        assert_eq!(press(&mut b, KeyCode::Enter), Action::QueueOpen(3));
    }
}
