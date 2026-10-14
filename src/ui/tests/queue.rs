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

#[test]
fn renders_the_schedule_form() {
    let mut view = queued_view();
    view.overlay = Some(Overlay::Queue(queue_form::Form {
        prompt: crate::todo::editor::Editor::new("Fix the login form"),
        at: crate::todo::editor::Editor::new("claude reset"),
        branch: crate::todo::editor::Editor::new("fix-login"),
        field: crate::queue::form::Field::At,
        zone: "Europe/Madrid".into(),
        place: "shop · root".into(),
        agent: "claude".into(),
        mode: "plan".into(),
        worktree: true,
        note: None,
        busy: false,
    }));
    insta::assert_snapshot!(render_sized(&view, 100, 24).backend());
}

#[test]
fn the_pending_count_stays_visible_with_no_project_open() {
    let mut view = view(&[]);
    view.queue_pending = 2;
    let areas = layout(AREA, Widths::default());
    assert!(row_text(&render(&view), areas.queue_button).contains("Queue 2"));
}
