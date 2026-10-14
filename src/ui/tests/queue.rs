use super::*;
use crate::list::Combo;
use crate::queue::form::{Choice, Control, Form, When};
use crate::queue::{Place, State};
use crate::todo::editor::Editor;

fn queued_view() -> View<'static> {
    let mut view = view(&["shop"]);
    view.has_project = true;
    view.queue_pending = 1;
    let entries = [
        (State::Pending, "pending", "#482 Fix the login form"),
        (State::Started { pane: 7, server: "test".into() }, "started · pane 7", "Update the docs"),
        (State::Failed { reason: "codex is not installed".into() }, "failed · codex is not installed", "Fix logout"),
        (State::Missed, "missed", "Review the tests"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (state, status, label))| crate::ui::queue::Row {
        id: i as u64 + 1,
        when: "2026-10-14 14:30".into(),
        label: label.into(),
        meta: "claude · worktree".into(),
        status: status.into(),
        state,
    })
    .collect();
    view.queue = Some(crate::ui::queue::View { entries, scroll: 0, pending: 1 });
    view
}

#[test]
fn renders_pending_started_failed_and_missed_in_wide_mode() {
    let view = queued_view();
    insta::assert_snapshot!(render_sized(&view, 140, 28).backend());
}

#[test]
fn renders_pending_started_failed_and_missed_in_compact_mode() {
    let view = queued_view();
    insta::assert_snapshot!(render_sized(&view, 70, 28).backend());
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
    let mut view = queued_view();
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
#[case::two_rows_short(16)]
#[case::one_row_short(17)]
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
fn a_hovered_entry_keeps_its_grey_text_readable() {
    let mut view = queued_view();
    let panel = full_layout(Rect::new(0, 0, 140, 28), Widths::default(), true, view.sidebar, false, false).changes;
    let row = crate::ui::queue::rows(panel, view.queue.as_ref().expect("queue")).item(0);
    view.hover = Some(Position::new(row.x + 3, row.y + 2));
    let t = render_sized(&view, 140, 28);
    let cell = &t.backend().buffer()[(row.x + 1, row.y)];
    assert_eq!((cell.bg, cell.fg), (DARK_HOVER, view.muted));
}

#[test]
fn the_queue_button_lines_up_with_files_and_is_as_wide_as_its_label() {
    let mut view = view(&["shop"]);
    view.has_project = true;
    view.queue_pending = 3;
    let areas = layout(AREA, Widths::default());
    let button = queue_button(&areas, 3);
    assert_eq!((button.x, button.width), (areas.files_button.x, 9));
    assert_eq!(row_text(&render(&view), button), " Queue 3 ");
}

#[test]
fn compact_mode_shows_the_queue_beside_issues_and_leaves_the_bar_alone() {
    let mut view = view(&["shop"]);
    (view.has_project, view.issues, view.queue_pending, view.nav) = (true, true, 2, Some(Nav::Workspaces));
    let area = Rect::new(0, 0, 46, 24);
    let areas = layout(area, Widths::default()).shown(view.nav);
    let t = render_sized(&view, 46, 24);
    let middle = |r: Rect| row_text(&t, Rect { y: r.y + 1, height: 1, ..r });
    let button = queue_button(&areas, 2);
    assert_eq!(middle(button), " Queue 2 ");
    assert!(middle(Rect { width: button.x - areas.issues.x, ..areas.issues }).contains("Issues"));
    assert!(!middle(areas.bar).contains('Q'));
}

#[test]
fn the_pending_count_stays_visible_with_no_project_open() {
    let mut view = view(&[]);
    view.queue_pending = 2;
    let areas = layout(AREA, Widths::default());
    assert!(row_text(&render(&view), areas.queue_button).contains("Queue 2"));
}

#[test]
fn the_panel_buttons_light_up_under_the_pointer() {
    let mut view = queued_view();
    let panel = full_layout(Rect::new(0, 0, 140, 28), Widths::default(), true, view.sidebar, false, false).changes;
    let rows = crate::ui::queue::rows(panel, view.queue.as_ref().expect("queue"));
    let (add, first) = (rows.button(), rows.item(0));
    for pos in [Position::new(add.x + 2, add.y), Position::new(first.right() - 6, first.y + 3)] {
        view.hover = Some(pos);
        assert_eq!(render_sized(&view, 140, 28).backend().buffer()[(pos.x, pos.y)].bg, Color::Cyan, "{pos:?}");
    }
}
