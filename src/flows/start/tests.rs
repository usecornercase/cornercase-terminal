use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::agents::Known;
use crate::flows::builtin;
use crate::flows::dialog::DIAGRAM;

const ROOM: Room = Room { width: 30, height: 4, wide: true };

fn listing() -> Listing {
    let mut builtins = builtin::flows(&[Known::Claude, Known::Codex], Some(Known::Claude));
    builtins.retain(|flow| flow.name != "Arena");
    let mut flows: Vec<Saved> = builtins.into_iter().map(|flow| Saved { flow, origin: Origin::BuiltIn }).collect();
    let mut mine = flows[2].flow.clone();
    mine.name = "Mine".into();
    flows.push(Saved { flow: mine, origin: Origin::User(PathBuf::from("/flows/mine.toml")) });
    let builtins = flows.iter().take(4).map(|saved| saved.flow.name.clone()).collect();
    Listing { flows, hidden: Vec::new(), broken: vec![(PathBuf::from("/flows/bad.toml"), "broken".into())], builtins }
}

fn start() -> Start {
    Start::new(7, "web-shop".into(), true, listing())
}

fn key(s: &mut Start, code: KeyCode) -> Action {
    s.key(KeyEvent::new(code, KeyModifiers::NONE), ROOM)
}

fn type_text(s: &mut Start, text: &str) {
    for c in text.chars() {
        key(s, KeyCode::Char(c));
    }
}

fn ready() -> Start {
    let mut s = start();
    type_text(&mut s, "Add dark mode");
    s
}

mod opening {
    use super::*;

    #[test]
    fn a_projects_flow_named_like_a_deleted_built_in_is_red_and_does_not_start() {
        let mut listing = listing();
        let mut project = listing.flows[0].flow.clone();
        project.name = "Arena".into();
        listing.flows.push(Saved { flow: project, origin: Origin::Project(PathBuf::from("/p/arena.toml")) });
        listing.builtins.push("Arena".into());
        let mut s = Start::new(7, "web-shop".into(), true, listing);
        let i = s.items().iter().position(|item| item.name == "Arena").expect("listed");
        assert!(s.items()[i].red, "{:?}", s.items()[i]);
        s.cursor = i;
        type_text(&mut s, "Add dark mode");
        assert_eq!(s.start(), Action::None);
        assert!(s.notice.is_some());
    }

    #[test]
    fn it_lists_every_flow_but_files_that_cannot_be_read() {
        let s = start();
        let names: Vec<String> = s.items().iter().map(|item| item.name.clone()).collect();
        assert_eq!(names, ["Full issue", "Cross review", "Quick", "Bug, test first", "Mine"]);
    }

    #[test]
    fn each_flow_shows_its_name_its_agents_and_its_description_or_its_first_problem() {
        let mut listing = listing();
        listing.flows[1].flow.stages.clear();
        listing.flows[4].flow.name = "Quick".into();
        let items = Start::new(7, "web-shop".into(), true, listing).items();
        let full = &items[0];
        assert_eq!(
            (full.name.as_str(), full.agents.as_str(), full.tag, full.red),
            ("Full issue", "5 agents", None, false)
        );
        assert!(full.line.starts_with("A coder, a reviewer and a security reviewer"), "{}", full.line);
        assert_eq!((items[1].line.as_str(), items[1].red), ("`coder` is in no stage", true));
        assert_eq!(items[4].tag, Some("edited"));
    }

    #[test]
    fn choosing_another_flow_scrolls_the_preview_back_to_the_top() {
        let mut s = start();
        s.scroll_preview(4, 10);
        assert_eq!(s.preview, 4);
        s.scroll_preview(20, 10);
        assert_eq!(s.preview, 10);
        s.select(2);
        assert_eq!(s.preview, 0);
    }

    #[test]
    fn the_first_flow_without_problems_is_selected() {
        let mut listing = listing();
        listing.flows[0].flow.stages.clear();
        let s = Start::new(7, "web-shop".into(), true, listing);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn the_first_built_in_flow_is_selected_wherever_it_sorts() {
        let mut listing = listing();
        listing.flows.sort_by(|a, b| a.flow.name.cmp(&b.flow.name));
        assert_ne!(listing.flows[0].flow.name, "Full issue");
        let s = Start::new(7, "web-shop".into(), true, listing);
        assert_eq!(s.selected().map(|saved| saved.flow.name.as_str()), Some("Full issue"));
    }

    #[test]
    fn the_prompt_has_the_keys_at_first() {
        assert_eq!(start().focus, Focus::Prompt);
    }

    #[test]
    fn an_issue_brings_its_prompt_and_branch() {
        let s = start().for_issue("#482 Returns".into(), "https://x/482", "issue-482-returns".into());
        assert_eq!((s.prompt.text(), s.branch.as_str()), ("https://x/482".to_string(), "issue-482-returns"));
        assert_eq!(s.title(), "Start a flow › #482 Returns");
    }

    #[test]
    fn the_heading_names_the_flow_and_the_project() {
        assert_eq!(start().heading(), "Full issue in web-shop, in a worktree of its own");
    }
}

mod branches {
    use super::*;

    #[test]
    fn the_branch_follows_the_first_words_of_the_prompt() {
        let mut s = start();
        type_text(&mut s, "Add a dark mode to the settings page, then make sure the contrast works");
        assert_eq!(s.branch, "add-a-dark-mode-to-the-settings-page");
    }

    #[test]
    fn a_typed_branch_stops_following_the_prompt() {
        let mut s = ready();
        key(&mut s, KeyCode::Tab);
        for _ in 0..4 {
            key(&mut s, KeyCode::Backspace);
        }
        type_text(&mut s, "theme");
        key(&mut s, KeyCode::BackTab);
        type_text(&mut s, " now");
        assert_eq!(s.branch, "add-dark-theme");
    }

    #[test]
    fn a_prompt_without_words_gives_the_default_branch() {
        let mut s = start();
        type_text(&mut s, "!!!");
        assert_eq!(s.branch, "flow");
    }

    #[test]
    fn an_issue_branch_does_not_follow_the_prompt() {
        let mut s = start().for_issue("#1".into(), "fix it", "issue-1-fix".into());
        type_text(&mut s, "please ");
        assert_eq!(s.branch, "issue-1-fix");
    }
}

mod keys {
    use super::*;

    #[test]
    fn tab_moves_from_the_list_to_the_prompt_to_the_branch() {
        let mut s = start();
        let mut seen = vec![s.focus];
        for _ in 0..3 {
            key(&mut s, KeyCode::Tab);
            seen.push(s.focus);
        }
        assert_eq!(seen, [Focus::Prompt, Focus::Branch, Focus::List, Focus::Prompt]);
    }

    #[test]
    fn enter_in_the_prompt_starts_a_new_line() {
        let mut s = ready();
        key(&mut s, KeyCode::Enter);
        type_text(&mut s, "with care");
        assert_eq!(s.prompt.text(), "Add dark mode\nwith care");
    }

    #[test]
    fn arrows_in_the_list_pick_a_flow() {
        let mut s = start();
        key(&mut s, KeyCode::BackTab);
        key(&mut s, KeyCode::Down);
        key(&mut s, KeyCode::Down);
        assert_eq!(s.selected().map(|saved| saved.flow.name.as_str()), Some("Quick"));
    }

    #[test]
    fn enter_in_the_branch_starts() {
        let mut s = ready();
        key(&mut s, KeyCode::Tab);
        assert!(matches!(key(&mut s, KeyCode::Enter), Action::Start { .. }));
    }

    #[test]
    fn esc_closes() {
        assert_eq!(key(&mut ready(), KeyCode::Esc), Action::Close);
    }

    #[test]
    fn a_paste_keeps_its_lines_in_the_prompt_and_goes_on_one_line_in_the_branch() {
        let mut s = start();
        s.paste("one\ntwo", ROOM);
        key(&mut s, KeyCode::Tab);
        s.paste("x\ny", ROOM);
        assert_eq!((s.prompt.text(), s.branch.as_str()), ("one\ntwo".to_string(), "one-twoxy"));
    }
}

mod starting {
    use super::*;

    #[test]
    fn start_hands_over_the_flow_the_prompt_and_the_branch() {
        let mut s = ready();
        s.select(2);
        let Action::Start { flow, task, branch, accounts } = s.press(START) else { panic!("not started") };
        assert_eq!((flow.name.as_str(), task.as_str(), branch.as_str()), ("Quick", "Add dark mode", "add-dark-mode"));
        assert!(accounts.is_empty(), "without accounts the run takes the accounts in use");
    }

    #[test]
    fn a_project_that_is_not_a_repository_cannot_start_a_flow() {
        let mut s = Start::new(7, "notes".into(), false, listing());
        type_text(&mut s, "Add dark mode");
        assert_eq!(s.press(START), Action::None);
        assert_eq!(s.notice.as_deref(), Some("a flow needs a project that is the root of a git repository"));
        assert_eq!(
            s.warnings()[0],
            "notes is not the root of a git repository, and a flow works in a worktree of its own"
        );
    }

    #[test]
    fn a_flow_with_problems_is_shown_but_not_started() {
        let mut listing = listing();
        listing.flows[0].flow.stages.clear();
        let mut s = Start::new(7, "web-shop".into(), true, listing);
        type_text(&mut s, "Add dark mode");
        s.select(0);
        assert!(s.items()[0].red);
        assert_eq!(s.press(START), Action::None);
        assert_eq!(s.warnings()[0], "`coder` is in no stage");
    }

    #[test]
    fn an_empty_prompt_or_branch_is_refused() {
        let mut s = start();
        assert_eq!(s.press(START), Action::None);
        assert_eq!(s.notice.as_deref(), Some("say what the team should do in the prompt"));
        type_text(&mut s, "!!!");
        key(&mut s, KeyCode::Tab);
        for _ in 0.."flow".len() {
            key(&mut s, KeyCode::Backspace);
        }
        assert_eq!(s.press(START), Action::None);
        assert_eq!(s.notice.as_deref(), Some("the run needs a branch for its worktree"));
    }

    #[test]
    fn a_project_flow_says_in_red_what_env_and_args_it_sets() {
        let mut listing = listing();
        let mut flow = listing.flows[4].flow.clone();
        flow.roles[0].env.insert("NODE_OPTIONS".into(), "--require ./x.js".into());
        flow.roles[1].args = vec!["--foo".into(), "a b".into()];
        listing.flows[4].flow = flow.clone();
        listing.flows.push(Saved { flow, origin: Origin::Project(PathBuf::from("/repo/.cornercase/flows/mine.toml")) });
        let mut s = Start::new(7, "web-shop".into(), true, listing);
        let risk = "this project's flow sets NODE_OPTIONS for `coder` and passes --foo 'a b' to `reviewer`; they run \
                    without asking, so check them";
        s.select(5);
        assert_eq!(s.warnings()[0], risk);
        s.select(4);
        assert!(!s.warnings().iter().any(|w| w.starts_with("this project's flow")));
    }

    #[test]
    fn edit_names_the_selected_flow() {
        let mut s = start();
        s.select(4);
        assert_eq!(s.press(EDIT), Action::Edit(Origin::User(PathBuf::from("/flows/mine.toml")), "Mine".into()));
    }

    #[test]
    fn coming_back_from_the_editor_selects_the_flow_edited_there() {
        let mut s = ready();
        let mut listing = listing();
        let copy =
            Saved { flow: listing.flows[0].flow.clone(), origin: Origin::User(PathBuf::from("/flows/copy.toml")) };
        listing.flows.push(copy);
        s.set_listing(listing, Some(&Origin::User(PathBuf::from("/flows/copy.toml"))));
        assert_eq!((s.cursor, s.prompt.text()), (5, "Add dark mode".to_string()));
    }

    #[test]
    fn coming_back_keeps_the_selection_when_nothing_was_edited() {
        let mut s = start();
        s.select(4);
        s.set_listing(listing(), None);
        assert_eq!(s.cursor, 4);
    }
}

mod accounts {
    use super::*;

    fn agent(kind: &str, active: &str) -> AgentAccounts {
        let names = ["home".to_string(), "work".to_string()];
        let labels = vec!["System".into(), "Personal".into(), "work".into()];
        AgentAccounts::new(kind.into(), &names, labels, active)
    }

    fn with_accounts() -> Start {
        let mut s = ready().with_accounts(vec![agent("claude", "work"), agent("codex", ""), agent("gemini", "home")]);
        s.focus = Focus::Branch;
        s
    }

    fn chosen(s: &mut Start) -> BTreeMap<String, String> {
        match s.press(START) {
            Action::Start { accounts, .. } => accounts,
            other => panic!("not started: {other:?}"),
        }
    }

    fn picked(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(kind, name)| ((*kind).to_string(), (*name).to_string())).collect()
    }

    fn team(s: &Start) -> Vec<&str> {
        s.shown_accounts().into_iter().map(|i| s.accounts[i].kind.as_str()).collect()
    }

    #[test]
    fn a_combo_per_agent_of_the_team_starts_on_the_account_in_use() {
        let mut s = with_accounts();
        assert_eq!(team(&s), ["claude", "codex"], "gemini is in no role of this flow");
        assert_eq!(s.accounts[0].combo.options(), ["System", "Personal", "work"]);
        assert_eq!(s.accounts[0].combo.label(), Some("work"));
        assert_eq!(chosen(&mut s), picked(&[("claude", "work"), ("codex", "")]));
    }

    #[test]
    fn an_account_in_use_that_is_gone_starts_on_the_system() {
        let s = ready().with_accounts(vec![agent("claude", "gone")]);
        assert_eq!(s.accounts[0].chosen(), "");
    }

    #[test]
    fn the_issues_pick_seeds_the_combos_and_unknown_names_are_left_alone() {
        let mut s = with_accounts();
        s.seed_accounts(&picked(&[("claude", "home"), ("codex", "gone")]));
        assert_eq!(chosen(&mut s), picked(&[("claude", "home"), ("codex", "")]));
    }

    #[test]
    fn another_flow_shows_the_combos_of_its_own_team() {
        let mut s = with_accounts();
        let only_claude = s.flows.iter().position(|saved| {
            std::iter::once(&saved.flow.orchestrator).chain(&saved.flow.roles).all(|role| role.agent == "claude")
        });
        let Some(i) = only_claude else { return };
        s.focus = Focus::Account(1);
        s.select(i);
        assert_eq!((team(&s), s.focus), (vec!["claude"], Focus::Branch));
    }

    #[test]
    fn tab_reaches_each_account_after_the_branch_and_goes_back_to_the_list() {
        let mut s = with_accounts();
        key(&mut s, KeyCode::Tab);
        assert_eq!(s.focus, Focus::Account(0));
        key(&mut s, KeyCode::Tab);
        assert_eq!(s.focus, Focus::Account(1));
        key(&mut s, KeyCode::Tab);
        assert_eq!(s.focus, Focus::List);
        key(&mut s, KeyCode::BackTab);
        assert_eq!(s.focus, Focus::Account(1));
        assert!(s.hint().starts_with("the codex account this team's codex agents start with"), "{}", s.hint());
    }

    #[test]
    fn without_combos_tab_skips_them() {
        let mut s = ready();
        s.focus = Focus::Branch;
        key(&mut s, KeyCode::Tab);
        assert_eq!(s.focus, Focus::List);
    }

    #[test]
    fn keys_pick_an_account_and_esc_closes_only_the_list() {
        let mut s = with_accounts();
        key(&mut s, KeyCode::Tab);
        key(&mut s, KeyCode::Enter);
        assert!(s.accounts_open());
        assert_eq!(key(&mut s, KeyCode::Esc), Action::None);
        assert!(!s.accounts_open());
        key(&mut s, KeyCode::Down);
        key(&mut s, KeyCode::Up);
        key(&mut s, KeyCode::Enter);
        assert_eq!(chosen(&mut s), picked(&[("claude", "home"), ("codex", "")]));
        assert_eq!(key(&mut s, KeyCode::Esc), Action::Close);
    }

    #[test]
    fn a_click_opens_one_list_and_a_choice_goes_to_that_combo() {
        let mut s = with_accounts();
        s.hit(Hit::Account(1), ROOM);
        assert_eq!((s.focus, s.accounts[1].combo.is_open()), (Focus::Account(1), true));
        s.hit(Hit::Choose(2), ROOM);
        assert!(!s.accounts_open());
        assert_eq!(chosen(&mut s), picked(&[("claude", "work"), ("codex", "work")]));
        s.hit(Hit::Account(0), ROOM);
        s.hit(Hit::Account(1), ROOM);
        assert_eq!((s.accounts[0].combo.is_open(), s.accounts[1].combo.is_open()), (false, true));
        s.hit(Hit::Branch, ROOM);
        assert_eq!((s.focus, s.accounts_open()), (Focus::Branch, false));
    }
}

mod waiting {
    use super::*;

    #[test]
    fn while_the_run_starts_nothing_changes_the_dialog() {
        let mut s = ready();
        s.waiting = true;
        assert_eq!(key(&mut s, KeyCode::Esc), Action::None);
        type_text(&mut s, "x");
        s.paste("y", ROOM);
        assert_eq!(s.hit(Hit::Button(CANCEL), ROOM), Action::None);
        assert_eq!(s.prompt.text(), "Add dark mode");
    }

    #[test]
    fn a_failure_is_said_and_the_dialog_answers_again() {
        let mut s = ready();
        s.waiting = true;
        s.failed("branch is checked out elsewhere");
        assert_eq!(s.notice.as_deref(), Some("the flow did not start: branch is checked out elsewhere"));
        assert_eq!(key(&mut s, KeyCode::Esc), Action::Close);
    }
}

mod mouse {
    use super::*;

    #[test]
    fn a_click_on_a_flow_selects_it() {
        let mut s = start();
        s.hit(Hit::Row(3), ROOM);
        assert_eq!((s.cursor, s.focus), (3, Focus::List));
    }

    #[test]
    fn a_click_in_the_prompt_places_the_cursor() {
        let mut s = start();
        s.paste("abc\ndef", ROOM);
        s.hit(Hit::Branch, ROOM);
        s.hit(Hit::Prompt { line: 1, col: 1 }, ROOM);
        type_text(&mut s, "X");
        assert_eq!(s.prompt.text(), "abc\ndXef");
    }

    #[test]
    fn a_narrow_dialog_shows_the_diagram_behind_a_button() {
        let mut s = start();
        let narrow = Room { wide: false, ..ROOM };
        assert_eq!(s.buttons(narrow), [EDIT, DIAGRAM, CANCEL, START]);
        s.hit(Hit::Button(DIAGRAM), narrow);
        assert!(s.diagram);
        assert_eq!(s.buttons(ROOM), [EDIT, CANCEL, START]);
    }

    #[test]
    fn the_wheel_scrolls_the_prompt_and_the_list() {
        let mut s = start();
        s.paste(&["line"; 10].join("\n"), ROOM);
        s.scroll_prompt(-3, ROOM);
        s.scroll_list(1);
        assert_eq!((s.prompt.scroll(), s.cursor), (3, 1));
    }
}
