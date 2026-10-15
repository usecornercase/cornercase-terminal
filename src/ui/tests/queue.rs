use super::*;
use crate::list::Combo;
use crate::queue::Place;
use crate::queue::form::{Choice, Control, Form, When};
use crate::todo::editor::Editor;

fn queue_tab() -> View<'static> {
    let row = |when: &str, label: &str, meta: &str, colour| IssueRow {
        key: when.into(),
        title: label.into(),
        meta: meta.into(),
        colour,
    };
    let items = vec![
        row("2026-10-14 14:30", "#482 Fix the login form", "pending · claude · shop › issue-482", None),
        row("2026-10-14 15:00", "Update the docs", "started · pane 7 · codex · shop", Some(Color::Green)),
        row("2026-10-14 16:00", "Fix logout", "failed · codex is not installed · codex · shop", Some(Color::Red)),
        row("2026-10-14 17:00", "Review the tests", "missed · claude · api", Some(Color::Yellow)),
    ];
    let issues = Issues {
        title: "Issues · shop".into(),
        tabs: vec!["All", "GitHub", "Queue"],
        tab: 2,
        toggles: Vec::new(),
        on: Vec::new(),
        body: IssuesBody::List { filter: String::new(), items, selected: Some(0), scroll: 0, empty: String::new() },
        note: None,
        hint: "enter edits it · start now runs it at once".into(),
        buttons: vec!["new", "start now", "remove", "clear started", "cancel"],
    };
    View { overlay: Some(Overlay::Issues(issues)), ..view(&["shop"]) }
}

#[test]
fn the_queue_tab_lists_every_entry_with_its_state_in_colour() {
    let t = render_sized(&queue_tab(), 120, 24);
    insta::assert_snapshot!(t.backend());
    let lines = lines(&t);
    let failed = lines.iter().position(|l| l.contains("Fix logout")).expect("the failed row");
    let x = lines[failed].find("failed").expect("its state");
    let x = u16::try_from(lines[failed][..x].chars().count()).expect("column");
    let y = u16::try_from(failed).expect("row");
    assert_eq!(t.backend().buffer()[(x, y)].fg, Color::Red);
}

fn form(prompt: &str, note: Option<&str>) -> Form {
    let places = vec![
        Choice { label: "shop · root".into(), place: Place::default(), worktree: true },
        Choice { label: "shop › main".into(), place: Place::default(), worktree: false },
    ];
    let mut form =
        Form::new(places, Some(0), vec!["auto".into(), "claude".into()], 1, "Europe/Madrid".into(), "2026-10-14");
    form.prompt = Editor::new(prompt);
    form.open_in.choose(1);
    form.branch = Editor::new("fix-login");
    form.mode = Combo::new(vec!["default".into(), "plan".into()], Some(1));
    form.focus = Control::When;
    form.error = note.map(str::to_string);
    form
}

fn lines(t: &Terminal<TestBackend>) -> Vec<String> {
    let area = t.backend().buffer().area;
    (area.y..area.bottom()).map(|y| row_text(t, Rect { y, height: 1, ..area })).collect()
}

fn shown(form: Form, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut view = view(&["shop"]);
    view.overlay = Some(Overlay::Queue(Box::new(form)));
    render_sized(&view, width, height)
}

#[test]
fn renders_the_schedule_form() {
    insta::assert_snapshot!(shown(form("Fix the login form", None), 100, 24).backend());
}

#[test]
fn renders_the_kinds_of_time_in_an_open_list() {
    let mut f = form("Fix the login form", None);
    f.when.open();
    insta::assert_snapshot!(shown(f, 100, 24).backend());
}

#[test]
fn a_date_shows_its_day_and_its_clock_and_a_tab_shows_no_branch() {
    let mut f = form("Fix the login form", None);
    f.when = When::On.combo();
    f.open_in.choose(0);
    let lines = lines(&shown(f, 100, 24));
    assert!(lines.iter().any(|l| l.contains("When      on ")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.contains("2026-10-14   09:00")), "{lines:#?}");
    assert!(!lines.iter().any(|l| l.contains("fix-login")), "{lines:#?}");
}

#[test]
fn the_open_list_counts_as_part_of_the_dialog() {
    let mut f = form("fix", None);
    f.place.open();
    let screen = Rect::new(0, 0, 100, 12);
    let list = queue_form::list(screen, &f).expect("an open list");
    assert_eq!(queue_form::area_with_list(screen, &f), queue_form::area(screen).union(list));
}

#[rstest]
#[case::two_rows_short(20)]
#[case::one_row_short(21)]
fn a_short_form_shows_its_error_above_the_buttons(#[case] height: u16) {
    let lines = lines(&shown(form("fix", Some("choose a time in the future")), 100, height));
    let note = lines.iter().position(|l| l.contains("choose a time in the future")).expect("the note shows");
    let buttons = lines.iter().position(|l| l.contains(" schedule ") && l.contains(" cancel ")).expect("the buttons");
    assert!(note < buttons, "{lines:#?}");
}

#[test]
fn a_tiny_form_keeps_its_rows_inside_its_border() {
    let lines = lines(&shown(form("fix", None), 100, 8));
    let bottom = lines.iter().rposition(|l| l.contains('╰')).expect("the bottom border");
    assert!(!lines[bottom].contains("Mode") && !lines[bottom].contains("Agent"), "{lines:#?}");
}

#[test]
fn a_prompt_with_line_breaks_stays_on_its_rows() {
    let lines = lines(&shown(form("first line\nsecond line", None), 100, 24));
    assert!(lines.iter().any(|l| l.contains("first line↵second line")), "{lines:#?}");
}

#[test]
fn a_workspace_says_it_opens_a_new_tab_without_offering_a_choice() {
    let mut f = form("fix", None);
    f.place.choose(1);
    let lines = lines(&shown(f, 100, 24));
    let open_in = lines.iter().find(|l| l.contains("Open in")).expect("the open in row");
    assert!(open_in.contains("Open in   new tab") && !open_in.contains('▾'), "{open_in}");
}
