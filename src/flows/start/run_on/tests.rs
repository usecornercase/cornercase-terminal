use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Style;

use super::*;
use crate::agents::Known;
use crate::flows::builtin;
use crate::flows::dialog::{CANCEL, EDIT};
use crate::flows::start::{Focus, Hit};
use crate::flows::start::{Room, START};
use crate::flows::store::{Listing, Origin, Saved};

const ROOM: Room = Room { width: 30, height: 4, wide: true };

fn target(name: &str) -> Target {
    Target { name: name.into(), icon: '☁', link: 1, why: None }
}

fn listing() -> Listing {
    let flows = builtin::flows(&[Known::Claude], Some(Known::Claude));
    let builtins = flows.iter().map(|flow| flow.name.clone()).collect();
    Listing {
        flows: flows.into_iter().map(|flow| Saved { flow, origin: Origin::BuiltIn }).collect(),
        builtins,
        ..Listing::default()
    }
}

fn start(run_on: Option<RunOn>) -> Start {
    let mut start = Start::new(7, "web-shop".into(), false, listing()).with_run_on(run_on);
    start.prompt = crate::editor::Editor::new("Add dark mode");
    start.typed();
    start
}

fn on_vps() -> Start {
    let mut start = start(Some(RunOn { targets: vec![target("vps")], ..RunOn::default() }));
    assert!(start.choose_machine(Some(0)));
    start
}

fn there(cloned: bool, listed: bool, lacking: Option<&str>) -> There {
    let place = "in its own worktree on vps, in ~/code/web-shop".into();
    There { place, cloned, listed, accounted: true, lacking: lacking.map(str::to_string), project: Some(1), era: 0 }
}

fn texts(spans: Option<Vec<Span<'static>>>) -> Option<String> {
    spans.map(|spans| spans.iter().map(|span| span.content.as_ref()).collect())
}

#[test]
fn has_no_row_without_machines() {
    let s = start(None);

    assert_eq!(texts(s.run_on_spans(Style::default())), None);
    assert_eq!((s.run_on_choices(), s.machine()), (0, None));
    assert_eq!(s.buttons(ROOM), [EDIT, CANCEL, START]);
}

#[test]
fn chooses_a_machine_once_and_says_what_it_lacks() {
    let mut s = on_vps();
    s.there = Some(there(true, true, Some("vps has no GitHub token for web-shop")));

    assert!(!s.choose_machine(Some(0)));
    assert_eq!(
        texts(s.run_on_spans(Style::default())).as_deref(),
        Some("○ this computer   ● ☁ vps · vps has no GitHub token for web-shop")
    );
    assert_eq!(s.run_on_choices(), 2);
}

#[test]
fn hides_edit_and_says_where_the_run_goes() {
    let mut s = on_vps();
    s.there = Some(there(true, true, None));

    assert_eq!(s.buttons(ROOM), [CANCEL, START]);
    assert!(s.heading().ends_with(" in its own worktree on vps, in ~/code/web-shop"), "{}", s.heading());
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
}

#[test]
fn asks_to_clone_the_project_when_it_is_not_there() {
    let mut s = on_vps();
    s.there = Some(there(false, false, None));

    assert_eq!(s.press(START), Action::CloneThere);
}

#[test]
fn sends_nothing_while_the_machine_lacks_something_or_has_not_listed_its_flows_and_accounts() {
    let mut s = on_vps();

    s.there = Some(there(true, true, Some("vps has no GitHub token for web-shop")));
    assert_eq!(s.press(START), Action::AskAgain);
    assert_eq!(s.notice.as_deref(), Some("vps has no GitHub token for web-shop"));

    s.there = Some(there(true, false, None));
    assert_eq!(s.press(START), Action::None);
    assert_eq!(s.notice.as_deref(), Some("reading the flows on vps…"));

    s.there = Some(There { accounted: false, ..there(true, true, None) });
    assert_eq!(s.press(START), Action::None);
    assert_eq!(s.notice.as_deref(), Some("reading the accounts on vps…"));
}

#[test]
fn starts_there_once_it_is_ready_even_outside_a_repository_here() {
    let mut s = on_vps();
    s.there = Some(there(true, true, None));

    assert!(matches!(s.press(START), Action::Start { .. }));
}

#[test]
fn a_click_on_the_row_chooses_and_says_so() {
    let mut s = start(Some(RunOn { targets: vec![target("vps")], ..RunOn::default() }));

    assert_eq!(s.hit(Hit::RunOn(Some(0)), ROOM), Action::RunOn);
    assert_eq!(s.hit(Hit::RunOn(Some(0)), ROOM), Action::None);
    s.waiting = true;
    assert_eq!(s.starting_there().as_deref(), Some("starting on vps…"));
    assert_eq!(s.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), ROOM), Action::None);
}

#[test]
fn starts_nothing_once_the_machine_is_gone_until_one_is_chosen() {
    let mut s = on_vps();
    s.there = Some(there(true, true, None));
    s.run_on = RunOn::kept(None, s.run_on.as_ref());

    assert_eq!(s.press(START), Action::None);
    assert_eq!(s.notice.as_deref(), Some("vps is no longer listed"));
    assert_eq!(s.hit(Hit::RunOn(None), ROOM), Action::RunOn);
    assert!(matches!(s.press(START), Action::None) && s.machine().is_none());
}

#[test]
fn keeps_to_the_machine_s_flows_once_the_machine_is_gone() {
    let mut s = on_vps();
    s.run_on = RunOn::kept(None, s.run_on.as_ref());

    assert_eq!(s.buttons(ROOM), [CANCEL, START]);
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert!(s.heading().ends_with(" · choose where it runs"), "{}", s.heading());
}

mod from_an_issue_on_a_machine {
    use super::*;

    fn from_an_issue() -> Start {
        let mut s = start(Some(RunOn { targets: vec![target("vps")], ..RunOn::default() }));
        s.asked = Some(("#7".into(), "github".into()));
        assert!(s.choose_machine(Some(0)));
        s.there = Some(there(true, true, None));
        s
    }

    fn takes_no_typing(s: &mut Start) {
        assert_eq!(s.hit(Hit::Prompt { line: 0, col: 0 }, ROOM), Action::None);
        s.focus = Focus::List;
        s.paste("more", ROOM);
        s.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), ROOM);

        assert_eq!((s.focus, s.prompt.text().as_str()), (Focus::Branch, "Add dark mode"));
    }

    #[test]
    fn says_the_machine_writes_the_prompt() {
        let s = from_an_issue();

        assert_eq!(s.templated().as_deref(), Some("from vps's template (the issue's URL by default)"));
        assert_eq!((s.focus, s.typing_prompt()), (Focus::Branch, false));
    }

    #[test]
    fn takes_no_typing_in_the_prompt() {
        let mut s = from_an_issue();

        takes_no_typing(&mut s);
    }

    #[test]
    fn starts_without_a_prompt_of_its_own() {
        let mut s = from_an_issue();
        s.prompt = crate::editor::Editor::default();

        assert!(matches!(s.press(START), Action::Start { .. }));
    }

    #[test]
    fn keeps_the_prompt_to_the_machine_once_the_machine_is_gone() {
        let mut s = from_an_issue();
        s.run_on = RunOn::kept(None, s.run_on.as_ref());

        assert_eq!(s.templated().as_deref(), Some("from the machine's template (the issue's URL by default)"));
        takes_no_typing(&mut s);
    }

    #[test]
    fn gives_the_prompt_back_on_this_computer() {
        let mut s = from_an_issue();

        assert_eq!(s.hit(Hit::RunOn(None), ROOM), Action::RunOn);

        assert_eq!(s.templated(), None);
        assert_eq!(s.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE), ROOM), Action::None);
        assert_eq!(s.focus, Focus::Prompt);
    }
}
