use ratatui::style::Modifier;

use super::*;
use crate::app::toast::COPIED;
use crate::host::clipboard;

const ANSWERED: &str = "Two are blocked.";

fn answered() -> Chat {
    let mut c = chat(ANSWER);
    c.say("which PRs are blocked?");
    c.settle();
    c.app.window_attached();
    c.app.show_bot(0);
    c
}

fn text_of(row: &ui::ChatRow) -> String {
    row.line.spans.iter().map(|span| span.content.as_ref()).collect()
}

fn start_of(app: &mut App, text: &str) -> Position {
    render(app, AREA);
    let areas = app.chat_geometry(AREA);
    let shown = app.shown_chat(AREA).expect("a chat");
    let body = ui::chat_body(areas.lines, shown.view.thinking.is_some());
    let at = shown.view.rows.iter().position(|row| text_of(row).contains(text)).expect(text);
    Position::new(body.x, ui::row_y(body, shown.view.rows.len(), false, at))
}

fn drag(app: &mut App, from: Position, to: Position) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), from);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to);
}

fn reversed(app: &mut App, pos: Position) -> bool {
    render(app, AREA).backend().buffer()[(pos.x, pos.y)].modifier.contains(Modifier::REVERSED)
}

fn copied(app: &mut App) -> String {
    use base64::Engine as _;
    let writes = app.take_host_writes();
    let last = String::from_utf8(writes.last().expect("a copy").clone()).expect("utf-8");
    let encoded = last.trim_start_matches("\u{1b}]52;c;").trim_end_matches('\u{7}');
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).expect("base64");
    String::from_utf8(decoded).expect("utf-8")
}

fn scroll(app: &App) -> usize {
    app.panels.bots.shown.as_ref().expect("shown").scroll
}

mod selecting {
    use super::*;

    #[test]
    fn a_drag_over_an_answer_copies_what_it_covers_and_keeps_it_lit() {
        let mut c = answered();
        let start = start_of(&mut c.app, ANSWERED);
        drag(&mut c.app, start, Position::new(start.x + 2, start.y));
        assert_eq!(c.app.take_host_writes(), vec![clipboard::osc52("Two")]);
        assert_eq!(toast(&c.app), Some(COPIED));
        assert!(reversed(&mut c.app, start) && reversed(&mut c.app, Position::new(start.x + 2, start.y)));
        assert!(!reversed(&mut c.app, Position::new(start.x + 3, start.y)));
    }

    #[test]
    fn a_drag_across_rows_copies_each_row_as_a_line() {
        let mut c = answered();
        let start = start_of(&mut c.app, ANSWERED);
        drag(&mut c.app, start, Position::new(start.x + 2, start.y - 1));
        let copied = copied(&mut c.app);
        assert!(copied.starts_with("s · ") && copied.ends_with("\nT"), "{copied}");
        assert_eq!(copied.lines().count(), 2, "{copied}");
    }

    #[test]
    fn the_wheel_while_dragging_scrolls_and_keeps_the_selection_going() {
        let mut c = answered();
        questions(&mut c, 30);
        c.app.show_bot(0);
        let start = start_of(&mut c.app, "question 29");
        mouse(&mut c.app, MouseEventKind::Down(MouseButton::Left), start);
        mouse(&mut c.app, MouseEventKind::ScrollUp, start);
        mouse(&mut c.app, MouseEventKind::ScrollUp, start);
        assert!(scroll(&c.app) > 0 && matches!(c.app.screen.drag, Some(Drag::Chat)));
        mouse(&mut c.app, MouseEventKind::Drag(MouseButton::Left), Position::new(start.x + 3, start.y));
        mouse(&mut c.app, MouseEventKind::Up(MouseButton::Left), Position::new(start.x + 3, start.y));
        let copied = copied(&mut c.app);
        assert!(copied.starts_with("question 28") && copied.lines().count() > 1, "{copied}");
    }

    #[test]
    fn a_click_on_the_lines_copies_nothing_and_clears_what_was_selected() {
        let mut c = answered();
        let start = start_of(&mut c.app, ANSWERED);
        drag(&mut c.app, start, Position::new(start.x + 2, start.y));
        c.app.take_host_writes();
        click(&mut c.app, start);
        assert_eq!(c.app.take_host_writes(), Vec::<Vec<u8>>::new());
        assert!(!reversed(&mut c.app, start));
    }

    #[test]
    fn lines_that_arrive_below_leave_the_selection_on_its_text() {
        let mut c = answered();
        let start = start_of(&mut c.app, ANSWERED);
        drag(&mut c.app, start, Position::new(start.x + 2, start.y));
        c.say("and the rest?");
        c.settle();
        let moved = start_of(&mut c.app, ANSWERED);
        assert!(moved.y < start.y, "the answer moved up");
        assert!(reversed(&mut c.app, moved) && !reversed(&mut c.app, start));
    }
}

mod keys {
    use super::*;

    #[test]
    fn page_up_and_page_down_scroll_the_chat_by_a_page_and_leave_the_draft_alone() {
        let mut c = chat(ANSWER);
        questions(&mut c, 60);
        c.app.window_attached();
        c.app.show_bot(0);
        render(&mut c.app, AREA);
        send_key(&mut c.app, KeyCode::Char('x'), KeyModifiers::NONE);
        let page = usize::from(ui::chat_body(c.app.chat_geometry(AREA).lines, false).height - 1);
        send_key(&mut c.app, KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(scroll(&c.app), page);
        send_key(&mut c.app, KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(scroll(&c.app), 2 * page);
        send_key(&mut c.app, KeyCode::PageDown, KeyModifiers::NONE);
        send_key(&mut c.app, KeyCode::PageDown, KeyModifiers::NONE);
        send_key(&mut c.app, KeyCode::PageDown, KeyModifiers::NONE);
        assert_eq!(scroll(&c.app), 0);
        assert_eq!(c.app.panels.bots.live[0].draft.text(), "x");
    }

    #[test]
    fn page_up_stops_at_the_oldest_line() {
        let mut c = chat(ANSWER);
        questions(&mut c, 1);
        c.app.window_attached();
        c.app.show_bot(0);
        for _ in 0..5 {
            send_key(&mut c.app, KeyCode::PageUp, KeyModifiers::NONE);
        }
        render(&mut c.app, AREA);
        assert_eq!(scroll(&c.app), 0);
    }
}
