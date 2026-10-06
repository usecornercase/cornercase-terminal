use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::{GAP, Rows, Zone, action_style, dim, draw_close, hovered_at as hovered, more_above, put};
use crate::todo::editor::{Editor, wrap};

pub const TEXT_X: u16 = 5;
const BUTTONS: u16 = 4;
const BUTTON_WIDTH: u16 = 3;
const TITLE: &str = "todo";
const NEW_TODO: &str = "+ new todo";
const CLEAR_DONE: &str = "clear done";
const PLACEHOLDER: &str = "enter adds, esc closes";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub items: Vec<Item>,
    pub scroll: usize,
    pub adding: Option<Editor>,
    pub light: bool,
    pub muted: Color,
    pub drag: Option<(u64, Option<usize>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: u64,
    pub text: String,
    pub done: bool,
    pub editing: Option<Editor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Close,
    ClearDone,
    Add,
    Field { item: Option<u64>, line: usize, col: usize },
    Check(u64),
    Delete(u64),
    Item { id: u64, row: Rect, line: usize, col: usize },
}

impl View {
    fn pending(&self) -> usize {
        self.items.iter().filter(|i| !i.done).count()
    }
}

impl Item {
    fn lines(&self, width: usize) -> usize {
        match &self.editing {
            Some(editor) => editor.lines(width).len(),
            None => wrap(&self.text, width).len(),
        }
    }
}

fn height(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn header(area: Rect) -> Rect {
    Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1).intersection(area)
}

pub fn close(area: Rect) -> Rect {
    let row = header(area);
    Rect::new(row.right().saturating_sub(BUTTON_WIDTH), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub fn clear_done(area: Rect, view: &View) -> Rect {
    if view.pending() == view.items.len() {
        return Rect::default();
    }
    let row = header(area);
    let w = height(CLEAR_DONE.chars().count()) + 2;
    Rect::new(close(area).x.saturating_sub(w + 1), row.y, w, 1).intersection(row)
}

pub fn text_width(area: Rect) -> usize {
    usize::from(area.width.saturating_sub(TEXT_X + BUTTONS).max(1))
}

pub fn rows(area: Rect, view: &View) -> Rows {
    let width = text_width(area);
    let y = area.y.saturating_add(1 + GAP).min(area.bottom());
    let list = Rect { y, height: area.bottom() - y, ..area };
    let heights = view.items.iter().map(|i| height(i.lines(width)).max(1)).collect();
    let button = view.adding.as_ref().map_or(1, |e| height(e.lines(width).len()));
    Rows { list, heights, button, scroll: view.scroll }
}

pub fn check(row: Rect) -> Rect {
    Rect::new(row.x + 1, row.y, 3, 1).intersection(row)
}

pub fn delete(row: Rect) -> Rect {
    Rect::new(row.right().saturating_sub(BUTTONS), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub fn spot(r: Rect, pos: Position) -> (usize, usize) {
    (usize::from(pos.y.saturating_sub(r.y)), usize::from(pos.x.saturating_sub(r.x + TEXT_X)))
}

pub fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    if clear_done(area, view).contains(pos) {
        return Some(Hit::ClearDone);
    }
    let layout = rows(area, view);
    let button = layout.button();
    if button.contains(pos) {
        let (line, col) = spot(button, pos);
        return Some(if view.adding.is_some() { Hit::Field { item: None, line, col } } else { Hit::Add });
    }
    let i = layout.at(pos)?;
    let (row, item) = (layout.item(i), &view.items[i]);
    let (line, col) = spot(row, pos);
    Some(if item.editing.is_some() {
        Hit::Field { item: Some(item.id), line, col }
    } else if check(row).contains(pos) {
        Hit::Check(item.id)
    } else if delete(row).contains(pos) {
        Hit::Delete(item.id)
    } else {
        Hit::Item { id: item.id, row, line, col }
    })
}

pub fn drop(area: Rect, view: &View, dragged: u64, pos: Position) -> Option<usize> {
    let from = view.items.iter().position(|i| i.id == dragged)?;
    let layout = rows(area, view);
    let before = match layout.zone(pos)? {
        Zone::Above => layout.first(),
        Zone::Row(j) if j > from => j + 1,
        Zone::Row(j) => j,
        Zone::Below => layout.end(),
    };
    let split = view.pending();
    Some(if view.items[from].done { before.clamp(split, view.items.len()) } else { before.min(split) })
}

fn surface(view: &View) -> Color {
    if view.light { super::LIGHT_SURFACE } else { super::DARK_SURFACE }
}

fn hover_fill(view: &View) -> Color {
    if view.light { super::LIGHT_HOVER } else { super::DARK_HOVER }
}

pub fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    let hover = hover.filter(|_| view.drag.is_none());
    draw_header(f.buffer_mut(), area, view, hover);
    let width = text_width(area);
    let layout = rows(area, view);
    let landing = view.drag.and_then(|(_, at)| at);
    let shown = landed(&layout, landing);
    let mut cursor = None;
    for (slot, row) in (shown.first()..shown.end()).map(|k| (k, shown.item(k))) {
        match slot_item(slot, landing) {
            Some(i) => {
                let dragged = view.drag.is_some_and(|(id, _)| id == view.items[i].id);
                cursor = cursor.or(draw_item(f.buffer_mut(), view, &view.items[i], row, width, hover, dragged));
            }
            None => draw_landing(f.buffer_mut(), row),
        }
    }
    let (above, below) = layout.hidden();
    let count = |range: std::ops::Range<usize>| (!range.is_empty()).then_some(range.len());
    super::draw_more(f, view.muted, [more_above(layout.list), shown.more_below()], count(above), count(below));
    let button = shown.button();
    if let Some(editor) = &view.adding {
        cursor = cursor.or(draw_field(f.buffer_mut(), view, editor, button, width, "+"));
    } else {
        let style = action_style(hovered(hover, button));
        put(f.buffer_mut(), button.x + 1, button.y, &format!(" {NEW_TODO} "), style, button.right());
    }
    if let Some(cursor) = cursor {
        f.set_cursor_position(cursor);
    }
}

fn draw_header(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) {
    let row = header(area);
    let x = put(buf, row.x, row.y, TITLE, Style::default().add_modifier(Modifier::BOLD), row.right());
    let pending = view.pending();
    if pending > 0 {
        put(buf, x + 1, row.y, &pending.to_string(), dim(view.muted), row.right());
    }
    let clear = clear_done(area, view);
    if !clear.is_empty() {
        let style =
            if hovered(hover, clear) { Style::default().fg(Color::Black).bg(Color::Cyan) } else { dim(view.muted) };
        put(buf, clear.x, clear.y, &format!(" {CLEAR_DONE} "), style, clear.right());
    }
    draw_close(buf, close(area), hover, view.muted);
}

fn landed(layout: &Rows, landing: Option<usize>) -> Rows {
    let Some(at) = landing else { return layout.clone() };
    let mut heights = layout.heights.clone();
    heights.insert(at.min(heights.len()), 1);
    let scroll = if at < layout.first() { layout.first() + 1 } else { layout.first() };
    Rows { heights, scroll, ..layout.clone() }
}

fn slot_item(slot: usize, landing: Option<usize>) -> Option<usize> {
    match landing {
        Some(at) if slot == at => None,
        Some(at) if slot > at => Some(slot - 1),
        _ => Some(slot),
    }
}

fn draw_landing(buf: &mut Buffer, row: Rect) {
    let line = "─".repeat(usize::from(row.width.saturating_sub(2)));
    put(buf, row.x + 1, row.y, &line, Style::default().fg(Color::Cyan), row.right());
}

fn text_area(row: Rect) -> Rect {
    let x = row.x.saturating_add(TEXT_X - 1).min(row.right());
    Rect { x, width: row.right().saturating_sub(x).saturating_sub(BUTTONS - 1), ..row }
}

fn draw_field(buf: &mut Buffer, view: &View, editor: &Editor, r: Rect, width: usize, mark: &str) -> Option<Position> {
    let field = text_area(r);
    buf.set_style(field, Style::default().bg(surface(view)));
    put(buf, r.x + 2, r.y, mark, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD), r.right());
    let text: Vec<char> = editor.text().chars().collect();
    let x = field.x + 1;
    if text.is_empty() {
        put(buf, x, r.y, PLACEHOLDER, dim(view.muted).bg(surface(view)), field.right());
    }
    for (line, range) in editor.lines(width).into_iter().enumerate() {
        let y = r.y.saturating_add(height(line));
        if y >= r.bottom() {
            break;
        }
        let part: String = text[range].iter().collect();
        put(buf, x, y, &part, Style::default().bg(surface(view)), field.right());
    }
    let (line, col) = editor.position(width);
    let at = Position::new(x.saturating_add(height(col)), r.y.saturating_add(height(line)));
    r.contains(at).then_some(at)
}

fn draw_item(
    buf: &mut Buffer,
    view: &View,
    item: &Item,
    row: Rect,
    width: usize,
    hover: Option<Position>,
    dragged: bool,
) -> Option<Position> {
    let lit = hovered(hover, row) && item.editing.is_none();
    let bg = if lit { Style::default().bg(hover_fill(view)) } else { Style::default() };
    if lit {
        buf.set_style(row, bg);
    }
    let box_style = if hovered(hover, check(row)) {
        bg.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else if item.done || dragged {
        bg.fg(view.muted)
    } else {
        bg
    };
    put(buf, row.x + 1, row.y, if item.done { "[x]" } else { "[ ]" }, box_style, row.right());
    if let Some(editor) = &item.editing {
        return draw_field(buf, view, editor, row, width, "");
    }
    let style = if item.done {
        bg.fg(view.muted).add_modifier(Modifier::CROSSED_OUT)
    } else if dragged {
        bg.fg(view.muted)
    } else {
        bg
    };
    let text: Vec<char> = item.text.chars().collect();
    for (line, range) in wrap(&item.text, width).into_iter().enumerate() {
        let y = row.y.saturating_add(height(line));
        if y >= row.bottom() {
            break;
        }
        let part: String = text[range].iter().collect();
        put(buf, row.x + TEXT_X, y, part.trim_end(), style, text_area(row).right());
    }
    if lit {
        let r = delete(row);
        let style = if hovered(hover, r) { bg.fg(Color::Red).add_modifier(Modifier::BOLD) } else { bg.fg(view.muted) };
        put(buf, r.x + 1, r.y, "×", style, r.right());
    }
    None
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use rstest::rstest;

    use super::*;

    const AREA: Rect = Rect::new(0, 0, 40, 14);

    fn item(id: u64, text: &str, done: bool) -> Item {
        Item { id, text: text.into(), done, editing: None }
    }

    fn view(items: Vec<Item>) -> View {
        View { items, scroll: 0, adding: None, light: false, muted: Color::DarkGray, drag: None }
    }

    fn list() -> View {
        view(vec![
            item(1, "fix the login bug in safari when the cookie expires", false),
            item(2, "write docs", false),
            item(3, "renew the domain", true),
        ])
    }

    fn render(view: &View, hover: Option<Position>) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
        t.draw(|f| draw(f, AREA, view, hover)).expect("draw");
        t
    }

    fn row(view: &View, i: usize) -> Rect {
        rows(AREA, view).item(i)
    }

    mod layout {
        use super::*;

        #[test]
        fn long_text_wraps_onto_more_rows() {
            let v = list();
            assert_eq!((text_width(AREA), row(&v, 0).height, row(&v, 1).height), (31, 2, 1));
        }

        #[test]
        fn clear_done_shows_only_with_done_items() {
            let v = view(vec![item(1, "a", false)]);
            assert_eq!(clear_done(AREA, &v), Rect::default());
            assert_eq!(clear_done(AREA, &list()).right() + 1, close(AREA).x);
        }
    }

    mod hits {
        use super::*;

        #[test]
        fn buttons_on_an_item_row() {
            let v = list();
            let r = row(&v, 0);
            assert_eq!(hit(AREA, &v, check(r).as_position()), Some(Hit::Check(1)));
            assert_eq!(hit(AREA, &v, delete(r).as_position()), Some(Hit::Delete(1)));
        }

        #[test]
        fn the_text_gives_its_line_and_column() {
            let v = list();
            let r = row(&v, 0);
            let pos = Position::new(r.x + TEXT_X + 4, r.y + 1);
            assert_eq!(hit(AREA, &v, pos), Some(Hit::Item { id: 1, row: r, line: 1, col: 4 }));
        }

        #[test]
        fn the_header_closes_and_clears() {
            let v = list();
            assert_eq!(hit(AREA, &v, close(AREA).as_position()), Some(Hit::Close));
            assert_eq!(hit(AREA, &v, clear_done(AREA, &v).as_position()), Some(Hit::ClearDone));
        }

        #[test]
        fn the_new_button_opens_a_field_that_then_takes_clicks() {
            let mut v = list();
            assert_eq!(hit(AREA, &v, rows(AREA, &v).button().as_position()), Some(Hit::Add));
            v.adding = Some(Editor::new("call"));
            let button = rows(AREA, &v).button();
            let pos = Position::new(button.x + TEXT_X + 2, button.y);
            assert_eq!(hit(AREA, &v, pos), Some(Hit::Field { item: None, line: 0, col: 2 }));
        }

        #[test]
        fn an_item_being_edited_takes_clicks_as_a_field() {
            let mut v = list();
            v.items[1].editing = Some(Editor::new("write docs"));
            let r = row(&v, 1);
            let pos = Position::new(r.x + TEXT_X + 1, r.y);
            assert_eq!(hit(AREA, &v, pos), Some(Hit::Field { item: Some(2), line: 0, col: 1 }));
        }
    }

    mod dropping {
        use super::*;

        fn four() -> View {
            view(vec![item(1, "a", false), item(2, "b", false), item(3, "c", false), item(4, "d", true)])
        }

        #[rstest]
        #[case::onto_a_row_below(1, 2, 3)]
        #[case::onto_a_row_above(3, 0, 0)]
        #[case::onto_itself(2, 1, 1)]
        #[case::onto_a_done_item_stays_with_the_pending_ones(1, 3, 3)]
        #[case::a_done_item_stays_with_the_done_ones(4, 0, 3)]
        fn the_row_under_the_pointer_gives_the_slot(#[case] id: u64, #[case] onto: usize, #[case] before: usize) {
            let v = four();
            assert_eq!(drop(AREA, &v, id, row(&v, onto).as_position()), Some(before));
        }

        #[test]
        fn outside_the_list_there_is_no_slot() {
            let v = four();
            assert_eq!(drop(AREA, &v, 1, close(AREA).as_position()), None);
        }
    }

    mod drawing {
        use super::*;

        #[test]
        fn the_list() {
            insta::assert_snapshot!(render(&list(), None).backend());
        }

        #[test]
        fn hovering_an_item_shows_its_delete_button() {
            let v = list();
            let r = row(&v, 0);
            let t = render(&v, Some(r.as_position()));
            let x = delete(r);
            assert_eq!(t.backend().buffer()[(x.x + 1, x.y)].symbol(), "×");
            assert_eq!(t.backend().buffer()[(r.x, r.y)].bg, crate::ui::DARK_HOVER);
        }

        #[test]
        fn a_new_item_field_shows_the_cursor() {
            let mut v = list();
            v.adding = Some(Editor::new("call the bank"));
            let mut t = render(&v, None);
            let button = rows(AREA, &v).button();
            insta::assert_snapshot!(t.backend());
            assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(button.x + TEXT_X + 13, button.y));
        }

        #[test]
        fn dragging_draws_a_line_where_it_lands() {
            let v = View { drag: Some((2, Some(0))), ..list() };
            insta::assert_snapshot!(render(&v, None).backend());
        }

        #[test]
        fn a_long_list_shows_how_many_are_hidden() {
            let v = view((0..20).map(|i| item(i, &format!("task {i}"), false)).collect());
            insta::assert_snapshot!(render(&v, None).backend());
        }
    }
}
