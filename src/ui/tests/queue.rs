use super::*;
use crate::queue::State;

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

fn form(prompt: &str, note: Option<&str>) -> queue_form::Form {
    queue_form::Form {
        prompt: crate::todo::editor::Editor::new(prompt),
        at: crate::todo::editor::Editor::new("claude reset"),
        branch: crate::todo::editor::Editor::new("fix-login"),
        field: crate::queue::form::Field::At,
        zone: "Europe/Madrid".into(),
        place: "shop · root".into(),
        agent: "claude".into(),
        mode: "plan".into(),
        worktree: true,
        note: note.map(str::to_string),
        busy: false,
    }
}

fn lines(t: &Terminal<TestBackend>) -> Vec<String> {
    let area = t.backend().buffer().area;
    (area.y..area.bottom()).map(|y| row_text(t, Rect { y, height: 1, ..area })).collect()
}

#[test]
fn renders_the_schedule_form() {
    let mut view = queued_view();
    view.overlay = Some(Overlay::Queue(form("Fix the login form", None)));
    insta::assert_snapshot!(render_sized(&view, 100, 24).backend());
}

#[rstest]
#[case::two_rows_short(20)]
#[case::one_row_short(21)]
fn a_short_form_shows_its_error_above_the_buttons(#[case] height: u16) {
    let mut view = view(&[]);
    view.overlay = Some(Overlay::Queue(form("fix", Some("choose a time in the future"))));
    let lines = lines(&render_sized(&view, 100, height));
    let note = lines.iter().position(|l| l.contains("choose a time in the future")).expect("the note shows");
    let buttons = lines.iter().position(|l| l.contains(" schedule ") && l.contains(" cancel ")).expect("the buttons");
    assert!(note < buttons, "{lines:#?}");
}

#[test]
fn a_tiny_form_keeps_its_reset_buttons_off_its_border() {
    let mut view = view(&[]);
    view.overlay = Some(Overlay::Queue(form("fix", None)));
    let lines = lines(&render_sized(&view, 100, 8));
    assert!(lines.iter().all(|l| !l.contains("reset")), "{lines:#?}");
}

#[test]
fn a_prompt_with_line_breaks_stays_on_its_rows() {
    let mut view = view(&[]);
    view.overlay = Some(Overlay::Queue(form("first line\nsecond line", None)));
    let lines = lines(&render_sized(&view, 100, 24));
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
