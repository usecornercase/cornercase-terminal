use super::panel::{NARROW, WIDE, at, drawn, full};
use super::*;

const CYAN: Color = Color::Cyan;

fn exited_view() -> View {
    let mut view = full(false);
    view.team[1].state = State::Exited;
    let flow = full_issue();
    let start = Instant::now();
    let progress = progress(&flow, &["coder", "reviewer", "coder"], start);
    let of = |role: &str| if role == "coder" { State::Exited } else { working(role) };
    view.diagram = Diagram::plan(&flow, MUTED, false).live(&progress, of, Ending::default(), start);
    view
}

fn drawn_over(view: &View, area: Rect, hover: Position) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(area.right(), area.bottom())).expect("terminal");
    t.draw(|f| draw(f, area, view, Some(hover))).expect("draw");
    t
}

fn mark_of_row(view: &View, area: Rect) -> Position {
    at(view, area, "▶ coder")
}

fn box_mark(t: &Terminal<TestBackend>) -> Position {
    let (x, y) = find(t, "╭─ coder").expect("the coder's box");
    let buf = t.backend().buffer();
    let x = (x..buf.area.width).find(|&x| buf[(x, y + 1)].symbol() == "▶").expect("the mark in the box");
    Position::new(x, y + 1)
}

#[test]
fn an_exited_member_shows_the_reopen_mark_and_says_exited() {
    let view = exited_view();
    let t = drawn(&view, WIDE);
    let mark = mark_of_row(&view, WIDE);
    assert_eq!(cell(&t, mark.x, mark.y).fg, CYAN);
    let name = cell(&t, mark.x + 2, mark.y);
    assert_eq!(name.fg, MUTED);
    assert!(!name.modifier.contains(Modifier::BOLD));
    let buf = t.backend().buffer();
    let row: String = (0..buf.area.width).map(|x| buf[(x, mark.y)].symbol()).collect();
    assert!(row.contains("exited") && row.contains("pane 5") && row.contains("fix/coder"));
    let at = row.find("exited").expect("exited");
    assert_eq!(cell(&t, u16::try_from(at).expect("column"), mark.y).fg, MUTED);
    insta::assert_snapshot!("panel_exited", t.backend());
}

#[test]
fn hovering_the_mark_bolds_and_underlines_it() {
    let view = exited_view();
    let mark = mark_of_row(&view, WIDE);
    let plain = drawn(&view, WIDE);
    assert!(!cell(&plain, mark.x, mark.y).modifier.contains(Modifier::UNDERLINED));
    let lit = drawn_over(&view, WIDE, mark);
    assert!(cell(&lit, mark.x, mark.y).modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
    let over_name = drawn_over(&view, WIDE, Position::new(mark.x + 2, mark.y));
    assert_eq!(cell(&over_name, mark.x + 2, mark.y).bg, Color::Indexed(235));
    assert!(!cell(&over_name, mark.x, mark.y).modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn its_mark_reopens_and_the_rest_of_its_row_goes_to_its_pane() {
    let view = exited_view();
    let mark = mark_of_row(&view, WIDE);
    assert_eq!(hit(WIDE, &view, mark), Some(Hit::Reopen));
    let coder = Some(Hit::Role("coder".into()));
    assert_eq!(hit(WIDE, &view, Position::new(mark.x + 2, mark.y)), coder);
    assert_eq!(hit(WIDE, &view, at(&view, WIDE, "exited")), coder);
}

#[test]
fn an_exited_box_says_exited_with_the_mark_and_takes_the_same_clicks() {
    let view = exited_view();
    let t = drawn(&view, WIDE);
    let mark = box_mark(&t);
    assert_eq!(cell(&t, mark.x, mark.y).fg, CYAN);
    let (x, y) = find(&t, "╭─ coder").expect("the box");
    assert_eq!(cell(&t, x + 3, y).fg, MUTED);
    let buf = t.backend().buffer();
    let line: String = (x..buf.area.width).map(|x| buf[(x, y + 1)].symbol()).collect();
    assert!(line.contains("exited"));
    assert_eq!(hit(WIDE, &view, mark), Some(Hit::Reopen));
    assert_eq!(hit(WIDE, &view, Position::new(x + 3, y)), Some(Hit::Role("coder".into())));
    let lit = drawn_over(&view, WIDE, mark);
    assert!(cell(&lit, mark.x, mark.y).modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
}

#[test]
fn a_narrow_panel_still_shows_the_mark_and_the_word() {
    let t = drawn(&exited_view(), NARROW);
    assert!(find(&t, "▶ coder").is_some());
    assert!(find(&t, "exited").is_some());
}
