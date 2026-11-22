use ratatui::text::Line as Text;

use super::*;
use crate::editor::Editor;

const TONE: Tone = Tone { muted: Color::DarkGray, surface: DARK_SURFACE };
const CHAT_W: u16 = 80;
const CHAT_H: u16 = 24;
const ROW_W: u16 = CHAT_W - 4;

fn markdown(source: &str) -> Vec<Text<'static>> {
    crate::highlight::markdown::render(source, usize::from(ROW_W))
}

fn you(text: &str, stamp: &str) -> ChatEntry {
    ChatEntry::You { text: text.into(), stamp: stamp.into() }
}

fn bot(text: &str) -> ChatEntry {
    ChatEntry::Bot { id: 9, head: "prs · 09:12".into(), body: markdown(text) }
}

fn steps(texts: &[&str], open: bool) -> ChatEntry {
    ChatEntry::Steps { first: 1, texts: texts.iter().map(ToString::to_string).collect(), open }
}

fn chat(entries: &[ChatEntry]) -> ChatView {
    ChatView {
        name: "prs".into(),
        detail: "claude · opus · high · home".into(),
        rows: chat_rows(entries, ROW_W, TONE),
        empty: false,
        thinking: None,
        news: false,
        busy: false,
        input: Editor::default(),
        focused: true,
        placeholder: "Message prs…".into(),
    }
}

fn drawn(chat: ChatView) -> Terminal<TestBackend> {
    drawn_hovering(chat, None)
}

fn drawn_hovering(chat: ChatView, hover: Option<Position>) -> Terminal<TestBackend> {
    let view = View { chat: Some(chat), hover, ..view(&[]) };
    let area = Rect::new(0, 0, CHAT_W, CHAT_H);
    let mut t = Terminal::new(TestBackend::new(CHAT_W, CHAT_H)).expect("test backend");
    t.draw(|f| bots::chat::draw(f, &view, view.chat.as_ref().expect("a chat"), area, 1)).expect("draw");
    t
}

fn all_lines() -> Vec<ChatEntry> {
    vec![
        you("which of my pull requests are blocked?", "09:12"),
        steps(&["gh pr view 482 --json reviews", "gh pr checks 490", "cornercase status --json"], false),
        bot("Two are **blocked**:\n\n- #482 waits for a review\n- #490 fails `test (macos)`"),
        you("and #501?", "held"),
        ChatEntry::Error { id: 9, text: "claude did not answer in time".into(), retry: true },
        ChatEntry::Notice("new conversation".into()),
    ]
}

#[test]
fn every_kind_of_line_is_drawn() {
    insta::assert_snapshot!(drawn(chat(&all_lines())).backend());
}

#[test]
fn a_run_of_steps_folds_to_the_last_three_under_a_count() {
    let texts: Vec<String> = (1..=7).map(|n| format!("step {n}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    insta::assert_snapshot!(drawn(chat(&[steps(&refs, false)])).backend());
}

#[test]
fn an_unfolded_run_shows_every_step() {
    let texts: Vec<String> = (1..=5).map(|n| format!("step {n}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let rows = chat_rows(&[steps(&refs, true)], ROW_W, TONE);
    assert_eq!(rows.len(), 5);
}

#[test]
fn the_thinking_line_has_the_spinner_the_time_the_steps_and_stop() {
    let mut view = chat(&all_lines()[..2]);
    view.thinking = Some(Thinking { text: "thinking · 42s · 3 steps".into(), stopping: false });
    view.busy = true;
    insta::assert_snapshot!(drawn(view).backend());
}

#[test]
fn a_stopping_turn_shows_no_stop_button() {
    let mut view = chat(&[]);
    view.thinking = Some(Thinking { text: "stopping…".into(), stopping: true });
    let areas = chat_areas(Rect::new(0, 0, CHAT_W, CHAT_H), 1, 1);
    assert!(stop_button(areas.lines, view.thinking.as_ref().expect("thinking")).is_empty());
}

#[test]
fn the_empty_state_says_what_a_bot_is_and_offers_four_examples() {
    let mut view = chat(&[]);
    view.rows = empty_rows(ROW_W, CHAT_H, TONE);
    view.empty = true;
    insta::assert_snapshot!(drawn(view).backend());
}

#[test]
fn a_long_markdown_answer_with_a_code_block_is_drawn() {
    let answer = "# Plan\n\nRun the **checks**:\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n1. first\n2. second\n\nSee [the docs](https://example.com/docs).";
    insta::assert_snapshot!(drawn(chat(&[you("how do I check?", "09:12"), bot(answer)])).backend());
}

#[test]
fn scrolled_up_shows_the_new_mark() {
    let mut view = chat(&all_lines());
    view.news = true;
    insta::assert_snapshot!(drawn(view).backend());
}

#[test]
fn the_input_grows_to_six_rows_and_then_scrolls() {
    let mut view = chat(&all_lines()[..1]);
    view.input = Editor::new(&(1..=9).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n"));
    insta::assert_snapshot!(drawn(view).backend());
    assert_eq!(input_rows(&view_input(9), Rect::new(0, 0, CHAT_W, CHAT_H)), 6);
}

fn view_input(lines: usize) -> Editor {
    Editor::new(&(1..=lines).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n"))
}

#[test]
fn a_you_bubble_is_at_most_eighty_percent_wide_and_right_aligned() {
    let long = "word ".repeat(60);
    let rows = chat_rows(&[you(&long, "09:12")], ROW_W, TONE);
    let widest = rows.iter().map(|row| row.line.width()).max().unwrap_or_default();
    assert!(widest <= usize::from(ROW_W));
    assert!(rows.iter().all(|row| row.line.spans[0].content.len() >= 12));
}

#[test]
fn a_link_in_an_answer_is_found_under_a_click() {
    let rows = chat_rows(&[bot("see https://example.com/a for it")], ROW_W, TONE);
    let row = rows.iter().find(|row| row.line.to_string().contains("example")).expect("the row");
    let col = row.line.to_string().find("example").expect("a column");
    assert_eq!(bots::chat::link_at(row, col).as_deref(), Some("https://example.com/a"));
    assert_eq!(bots::chat::link_at(row, 0), None);
}

mod whole_address {
    use super::*;

    const SHORT: &str = "https://example.com/a";
    const LONG: &str = "https://example.com/a/b/c";

    #[test]
    fn an_address_drawn_whole_is_kept_even_when_a_longer_one_starts_with_it() {
        assert_eq!(whole_address(SHORT, [LONG, &format!("see {SHORT}.")]).as_deref(), Some(SHORT));
    }

    #[test]
    fn a_piece_cut_by_the_wrap_gives_the_newest_answer_that_holds_it_whole() {
        let older = "https://example.com/a/older";
        let found = whole_address("https://example.com/a/", [&format!("([docs]({LONG}))") as &str, older]);
        assert_eq!(found.as_deref(), Some(LONG));
    }

    #[test]
    fn a_piece_no_answer_holds_finds_nothing() {
        assert_eq!(whole_address("https://other.dev/x", [LONG]), None);
    }

    #[rstest]
    #[case::bold("**https://example.com/a**", SHORT)]
    #[case::italic_then_a_full_stop("_https://example.com/a_.", SHORT)]
    #[case::bold_italic("***https://example.com/a***", SHORT)]
    #[case::strikethrough("~~https://example.com/a~~", SHORT)]
    #[case::an_asterisk_of_the_address_itself("https://example.com/a*", "https://example.com/a*")]
    fn emphasis_around_an_address_is_not_part_of_it(#[case] word: &str, #[case] expected: &str) {
        assert_eq!(whole_address(SHORT, [word]).as_deref(), Some(expected));
    }
}

#[test]
fn the_retry_button_is_only_on_the_error_row() {
    let rows = chat_rows(&[ChatEntry::Error { id: 3, text: "boom".into(), retry: true }], ROW_W, TONE);
    assert_eq!(rows[0].hit.as_ref().map(|(hit, _)| hit.clone()), Some(RowHit::Retry(3)));
}

#[test]
fn a_hit_test_finds_the_controls_and_the_rows() {
    let pane = Rect::new(0, 0, CHAT_W, CHAT_H);
    let view = chat(&all_lines());
    let areas = chat_areas(pane, 1, 1);
    let at = |r: Rect| Position::new(r.x + 1, r.y);
    assert_eq!(chat_hit(&areas, &view, at(areas.send)), Some(ChatHit::Send));
    assert_eq!(chat_hit(&areas, &view, at(areas.menu)), Some(ChatHit::Menu));
    assert_eq!(chat_hit(&areas, &view, at(areas.new_conversation)), Some(ChatHit::NewConversation));
    assert!(matches!(chat_hit(&areas, &view, at(areas.field)), Some(ChatHit::Input(0, _))));
}

#[test]
fn an_example_in_the_empty_state_is_hit() {
    let pane = Rect::new(0, 0, CHAT_W, CHAT_H);
    let mut view = chat(&[]);
    view.rows = empty_rows(ROW_W, CHAT_H, TONE);
    view.empty = true;
    let areas = chat_areas(pane, 1, 1);
    let top = row_y(areas.lines, view.rows.len(), true, view.rows.len() - 1);
    let hit = chat_hit(&areas, &view, Position::new(areas.lines.x + 30, top));
    assert_eq!(hit, Some(ChatHit::Row(RowHit::Example(3))));
}

#[test]
fn a_hovered_example_fills_cyan_and_the_others_keep_the_surface() {
    let mut view = chat(&[]);
    view.rows = empty_rows(ROW_W, CHAT_H, TONE);
    view.empty = true;
    let areas = chat_areas(Rect::new(0, 0, CHAT_W, CHAT_H), 1, 1);
    let last = row_y(areas.lines, view.rows.len(), true, view.rows.len() - 1);
    let first = row_y(areas.lines, view.rows.len(), true, view.rows.len() - 7);
    let x = areas.lines.x + 30;
    let t = drawn_hovering(view, Some(Position::new(x, last)));
    let buffer = t.backend().buffer();
    assert_eq!(buffer[(x, last)].bg, Color::Cyan);
    assert_eq!(buffer[(x, first)].bg, DARK_SURFACE);
}

#[test]
fn the_send_button_fills_once_there_is_a_draft() {
    let areas = chat_areas(Rect::new(0, 0, CHAT_W, CHAT_H), 1, 1);
    let at = (areas.send.x + 1, areas.input_text.y);
    let empty = drawn(chat(&[]));
    assert_eq!(empty.backend().buffer()[at].bg, DARK_SURFACE);
    let mut typed = chat(&[]);
    typed.input = Editor::new("hi");
    assert_eq!(drawn(typed).backend().buffer()[at].bg, Color::Cyan);
}

#[test]
fn a_wide_pane_keeps_the_chat_in_a_centred_column() {
    let areas = chat_areas(Rect::new(0, 0, 300, CHAT_H), 1, 1);
    assert!(areas.lines.width <= 110, "{areas:?}");
    assert_eq!(areas.lines.x, 300 - areas.lines.right());
    assert_eq!(areas.input.x, areas.lines.x);
}

#[rstest]
#[case::eight_rows(8, 6)]
#[case::five_rows(5, 3)]
#[case::six_rows(6, 2)]
fn a_low_pane_keeps_the_draft_inside_the_band(#[case] height: u16, #[case] rows: u16) {
    let pane = Rect::new(0, 0, 60, height);
    let areas = chat_areas(pane, rows, 1);
    assert!(areas.input_text.bottom() < areas.input.bottom(), "{areas:?}");
    assert!(areas.input_text.y > areas.input.y, "{areas:?}");
    let mut view = chat(&[]);
    view.input = Editor::new(&(1..=rows).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n"));
    let mut t = Terminal::new(TestBackend::new(60, height)).expect("test backend");
    let shown = View { chat: Some(view), ..super::view(&[]) };
    t.draw(|f| bots::chat::draw(f, &shown, shown.chat.as_ref().expect("a chat"), pane, 1)).expect("draw");
    let cursor = t.get_cursor_position().expect("a cursor");
    assert!(cursor.y < areas.input.bottom() - 1, "{cursor:?} {areas:?}");
}

#[test]
fn a_tall_pane_leaves_a_row_above_the_field_and_a_low_one_does_not() {
    let tall = chat_areas(Rect::new(0, 0, CHAT_W, CHAT_H), 1, 1);
    assert_eq!(tall.input.y - tall.lines.bottom(), 1);
    let low = chat_areas(Rect::new(0, 0, CHAT_W, 10), 1, 1);
    assert_eq!(low.input.y, low.lines.bottom());
}

#[test]
fn the_card_lists_the_bot_and_its_folder_is_a_target() {
    let card = BotCard {
        name: "prs".into(),
        rows: vec![
            CardRow { folder: true, ..CardRow::new("folder", "~/.local/state/cornercase/bots/prs") },
            CardRow::new("agent", "Claude Code"),
            CardRow::new("model", "opus"),
            CardRow { red: true, ..CardRow::new("access", "full access") },
            CardRow::new("made", "2026-11-14"),
        ],
    };
    let v = View { bot_card: Some(card.clone()), has_project: true, ..view(&["cornercase"]) };
    insta::assert_snapshot!(render(&v).backend());
    let a = areas();
    assert_eq!(card_folder(a.workspaces_list, &card).y, a.workspaces_list.y + 1);
}

fn long_card() -> BotCard {
    BotCard {
        name: "prs".into(),
        rows: vec![
            CardRow { folder: true, ..CardRow::new("folder", "~/bots/prs") },
            CardRow::new("agent", "Claude Code"),
            CardRow::new("model", "opus"),
            CardRow::new("effort", "high"),
            CardRow { red: true, ..CardRow::new("access", "full access") },
            CardRow::new("made", "2026-11-14"),
        ],
    }
}

#[test]
fn a_card_in_a_short_column_says_how_many_rows_it_hides() {
    let v = View { bot_card: Some(long_card()), has_project: true, ..view(&["cornercase"]) };
    let (width, height) = (100, 12);
    let t = render_sized(&v, width, height);
    let text = format!("{}", t.backend());
    assert!(text.contains("↓ 3 more") && text.contains("folder"), "{text}");
    insta::assert_snapshot!(t.backend());
}

#[test]
fn the_folder_target_follows_the_rows_that_are_drawn() {
    let card = long_card();
    let list = Rect::new(0, 3, 25, 2);
    assert_eq!(card_folder(list, &card).y, 4);
    let none = Rect::new(0, 3, 25, 1);
    assert!(card_folder(none, &card).is_empty());
}

fn run_block() -> RunBlock {
    RunBlock {
        run: 12,
        head: "↻ Review PRs · 09:00".into(),
        first: 4,
        steps: vec!["cornercase status --json".into(), "gh pr list".into()],
        open: false,
        answer: Some((5, "prs · 09:05".into(), markdown("Two are **blocked**:\n\n- #482 waits for a review"))),
        error: None,
        again: false,
        end: None,
        answering: false,
    }
}

#[test]
fn a_run_is_one_block_even_with_a_chat_turn_between_its_lines() {
    let entries = [you("hi", "09:01"), ChatEntry::Run(run_block()), bot("Fine."), steps(&["ls"], false)];
    let t = drawn(chat(&entries));
    insta::assert_snapshot!(t.backend());
    let text = (0..CHAT_H).map(|y| row_text(&t, Rect::new(0, y, CHAT_W, 1))).collect::<Vec<_>>().join("\n");
    assert!(text.contains("↻ Review PRs · 09:00") && text.contains("+2 steps"), "{text}");
    assert!(!text.contains("▸ gh pr list"), "the run's steps are folded");
}

#[test]
fn an_unfolded_run_shows_its_steps() {
    let rows = entry_rows(&ChatEntry::Run(RunBlock { open: true, ..run_block() }), ROW_W, TONE);
    let text: Vec<String> =
        rows.iter().map(|row| row.line.spans.iter().map(|s| s.content.as_ref()).collect()).collect();
    assert!(text.iter().any(|row| row.starts_with("▸ gh pr list")), "{text:?}");
}

#[test]
fn a_failed_run_offers_run_again() {
    let block = RunBlock { answer: None, error: Some("claude is not installed".into()), again: true, ..run_block() };
    let rows = entry_rows(&ChatEntry::Run(block), ROW_W, TONE);
    let last = rows.last().expect("a row");
    let text: String = last.line.spans.iter().map(|span| span.content.as_ref()).collect();
    assert!(text.ends_with(" run again "), "{text}");
    assert_eq!(last.hit.as_ref().map(|(hit, _)| hit.clone()), Some(RowHit::RunAgain(12)));
    let without = RunBlock { again: false, ..run_block() };
    let rows = entry_rows(&ChatEntry::Run(RunBlock { answer: None, error: Some("x".into()), ..without }), ROW_W, TONE);
    assert!(rows.iter().all(|row| row.hit.is_none() || matches!(row.hit, Some((RowHit::Unfold(_), _)))));
}

#[test]
fn a_run_that_is_answering_says_so() {
    let block = RunBlock { answer: None, answering: true, ..run_block() };
    let rows = entry_rows(&ChatEntry::Run(block), ROW_W, TONE);
    let text: String = rows.last().expect("a row").line.spans.iter().map(|span| span.content.as_ref()).collect();
    assert_eq!(text, "answering…");
}

#[test]
fn a_routine_takes_two_card_rows_and_its_time_survives_the_cut() {
    let card = BotCard {
        name: "prs".into(),
        rows: vec![
            CardRow::new("agent", "Claude Code"),
            CardRow::new("routine", "Review PRs"),
            CardRow::new("", "next Mon 09:00 · ↻ weekday 09:00"),
            CardRow::new("routine", "Nightly digest"),
            CardRow::new("", "paused · ↻ day 02:00"),
        ],
    };
    let v = View { bot_card: Some(card), has_project: true, ..view(&["cornercase"]) };
    let t = render(&v);
    let text: String = (0..t.backend().buffer().area.height)
        .map(|y| row_text(&t, Rect::new(0, y, t.backend().buffer().area.width, 1)))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("routine  Review PRs") && text.contains("next Mon 09:0"), "{text}");
    assert!(text.contains("Nightly digest") && text.contains("paused"), "{text}");
}

#[test]
fn a_fold_never_ends_on_a_routines_name_without_its_time_row() {
    let routine = |name: &str, when: &str| [CardRow::new("routine", name), CardRow::new("", when)];
    let rows: Vec<CardRow> = [routine("Review PRs", "next 09:00 · ↻ 2h"), routine("Nightly", "paused · ↻ day 02:00")]
        .into_iter()
        .flatten()
        .collect();
    let card = BotCard { name: "prs".into(), rows };
    for height in 8..16 {
        let v = View { bot_card: Some(card.clone()), has_project: true, ..view(&["cornercase"]) };
        let t = render_sized(&v, 100, height);
        let text: Vec<String> = (0..height).map(|y| row_text(&t, Rect::new(0, y, 100, 1))).collect();
        for (i, line) in text.iter().enumerate() {
            if line.contains("│ routine") || line.contains("routine  ") {
                let below = text.get(i + 1).map_or("", String::as_str);
                assert!(below.contains("next") || below.contains("paused"), "height {height}: {text:#?}");
            }
        }
    }
}

#[test]
fn the_header_has_routines_left_of_new_conversation() {
    let t = drawn(chat(&[]));
    let header = row_text(&t, Rect::new(0, 0, CHAT_W, 1));
    let (routines, new) = (header.find(" routines ").expect("routines"), header.find("new conversation").expect("new"));
    assert!(routines + " routines ".len() < new, "{header}");
    let areas = chat_areas(Rect::new(0, 0, CHAT_W, CHAT_H), 1, 1);
    assert_eq!((areas.routines.width, areas.routines.right() + 1), (10, areas.new_conversation.x));
}

#[test]
fn a_narrow_header_leaves_routines_to_the_menu() {
    let narrow = chat_areas(Rect::new(0, 0, 40, 10), 1, 1);
    assert!(narrow.routines.is_empty(), "{narrow:?}");
    assert!(!narrow.new_conversation.is_empty());
    let wide = chat_areas(Rect::new(0, 0, 60, 10), 1, 1);
    assert!(wide.routines.x >= wide.header.x + 12, "the name keeps its room: {wide:?}");
    let t = drawn_in(chat(&[]), Rect::new(0, 0, 40, 10));
    assert!(!row_text(&t, Rect::new(0, 0, 40, 1)).contains("routines"));
}

fn drawn_in(chat: ChatView, area: Rect) -> Terminal<TestBackend> {
    let view = View { chat: Some(chat), ..view(&[]) };
    let mut t = Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
    t.draw(|f| bots::chat::draw(f, &view, view.chat.as_ref().expect("a chat"), area, 1)).expect("draw");
    t
}

#[test]
fn chat_hit_finds_routines() {
    let view = chat(&all_lines());
    let areas = chat_areas(Rect::new(0, 0, CHAT_W, CHAT_H), 1, 1);
    let at = Position::new(areas.routines.x + 1, areas.routines.y);
    assert_eq!(chat_hit(&areas, &view, at), Some(ChatHit::Routines));
    let none = chat_areas(Rect::new(0, 0, 40, 10), 1, 1);
    assert_ne!(chat_hit(&none, &view, Position::new(0, 0)), Some(ChatHit::Routines));
}

#[test]
fn a_routine_row_of_the_card_is_a_target_on_both_its_rows() {
    let routine = |label, text: &str| CardRow { routine: Some(7), ..CardRow::new(label, text) };
    let card = BotCard {
        name: "prs".into(),
        rows: vec![
            CardRow::new("agent", "Claude Code"),
            routine("routine", "Review PRs"),
            routine("", "next Mon 09:00 · ↻ weekday 09:00"),
        ],
    };
    let list = Rect::new(0, 3, 25, 6);
    let at = |row: u16| Position::new(list.x + 3, list.y + 1 + row);
    assert_eq!(card_routine(list, &card, at(0)), None);
    assert_eq!(card_routine(list, &card, at(1)), Some(7));
    assert_eq!(card_routine(list, &card, at(2)), Some(7));
    assert_eq!(card_routine(list, &card, at(3)), None);
    let short = Rect::new(0, 3, 25, 3);
    assert_eq!(card_routine(short, &card, Position::new(3, 6)), None, "a hidden row is not a target");
    let a = areas();
    let hover = Position::new(a.workspaces_list.x + 4, a.workspaces_list.y + 2);
    let make = |hover| View { bot_card: Some(card.clone()), has_project: true, hover, ..view(&["cornercase"]) };
    let colour = |view: &View, pos: Position| render(view).backend().buffer()[pos].fg;
    let value = Position::new(a.workspaces_list.x + 1 + 9, a.workspaces_list.y + 2);
    assert_eq!(colour(&make(Some(hover)), value), Color::Cyan);
    assert_ne!(colour(&make(None), value), Color::Cyan);
}

fn two_routines(first: &str) -> BotCard {
    let routine = |id, label, text: &str| CardRow { routine: Some(id), ..CardRow::new(label, text) };
    BotCard {
        name: "prs".into(),
        rows: vec![
            CardRow::new("agent", "Claude Code"),
            routine(7, "routine", first),
            routine(7, "", "next Mon 09:00"),
            routine(8, "routine", "Nightly"),
            routine(8, "", "next Tue 09:00"),
        ],
    }
}

fn card_view(card: &BotCard, hover: Option<Position>, overlay: Option<Overlay>) -> View<'_> {
    View { bot_card: Some(card.clone()), has_project: true, hover, overlay, ..view(&["cornercase"]) }
}

#[test]
fn a_hovered_routine_shows_its_menu_button_at_the_end_of_its_first_row() {
    let card = two_routines("Review PRs");
    let a = areas();
    let list = a.workspaces_list;
    let menu = card_menu(list, &card, 7, 1);
    assert_eq!(menu.y, list.y + 2);
    assert_eq!(menu, row_menu_button(Rect::new(list.x + 1, menu.y, list.width - 1, 1), 1));
    let glyph = Position::new(menu.right() - 1, menu.y);
    let cell = |view: &View, pos: Position| render(view).backend().buffer()[pos].clone();
    assert_eq!(cell(&card_view(&card, None, None), glyph).symbol(), " ");
    let second = Position::new(list.x + 4, list.y + 3);
    let shown = cell(&card_view(&card, Some(second), None), glyph);
    assert_eq!((shown.symbol(), shown.fg), (ROW_MENU_ICON, Color::DarkGray));
    let on = cell(&card_view(&card, Some(glyph), None), glyph);
    assert_eq!((on.symbol(), on.fg), (ROW_MENU_ICON, Color::Cyan));
    let other = Position::new(menu.right() - 1, list.y + 4);
    assert_eq!(cell(&card_view(&card, Some(second), None), other).symbol(), " ");
    let menu_overlay = Overlay::Menu { at: second, items: vec!["pause".into()], disabled: Vec::new() };
    assert_eq!(cell(&card_view(&card, Some(second), Some(menu_overlay)), glyph).symbol(), " ");
    let short = Rect::new(list.x, list.y, list.width, 3);
    assert!(card_menu(short, &card, 7, 1).is_empty());
}

#[test]
fn the_button_is_where_the_view_draws_it_and_nowhere_else() {
    let card = two_routines("Review PRs");
    let list = areas().workspaces_list;
    let menu = card_menu(list, &card, 7, 1);
    let second = Position::new(list.x + 4, list.y + 3);
    assert_eq!(card_buttons(&card_view(&card, Some(second), None), list, 1), [(7, menu)]);
    assert_eq!(card_buttons(&card_view(&card, None, None), list, 1).len(), 0);
    let covered = Overlay::Menu { at: second, items: vec!["pause".into()], disabled: Vec::new() };
    assert_eq!(card_buttons(&card_view(&card, Some(second), Some(covered)), list, 1).len(), 0);
}

#[test]
fn a_compact_card_shows_every_routines_menu_button_without_hover() {
    let routine = |id, label, text: &str| CardRow { routine: Some(id), ..CardRow::new(label, text) };
    let card = BotCard {
        name: "prs".into(),
        rows: vec![
            routine(7, "routine", &"x".repeat(40)),
            routine(7, "", "next Mon 09:00"),
            routine(8, "routine", "Nightly"),
            routine(8, "", "next Tue 09:00"),
        ],
    };
    let list = Rect::new(0, 3, 60, 8);
    let view = card_view(&card, None, None);
    let buttons = card_buttons(&view, list, 2);
    assert_eq!(buttons.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [7, 8]);
    assert!(buttons.iter().all(|(_, rect)| !rect.is_empty() && rect.height == 2));
    assert_eq!(card_buttons(&view, list, 1).len(), 0, "a wide card still shows it only under the pointer");
}

#[test]
fn a_hovered_routine_cuts_its_name_before_its_menu_button_and_no_other_does() {
    let card = two_routines(&"x".repeat(40));
    let card = BotCard {
        rows: card
            .rows
            .into_iter()
            .map(|row| if row.value == "Nightly" { CardRow { value: "y".repeat(40), ..row } } else { row })
            .collect(),
        ..card
    };
    let a = areas();
    let list = a.workspaces_list;
    let menu = card_menu(list, &card, 7, 1);
    let hover = Position::new(list.x + 4, list.y + 3);
    let at = |x: u16, y: u16| {
        render(&card_view(&card, Some(hover), None)).backend().buffer()[Position::new(x, y)].symbol().to_string()
    };
    assert_eq!([at(menu.x - 2, menu.y), at(menu.x - 1, menu.y)], ["…", " "]);
    let other = list.y + 4;
    assert_eq!([at(menu.x - 2, other), at(menu.x, other)], ["y", "y"], "the other routine's name is not cut");
    let resting = render(&card_view(&card, None, None));
    assert_ne!(resting.backend().buffer()[Position::new(menu.x, menu.y)].symbol(), " ");
}

fn agent_line(kind: ChatAgentKind, text: &str, body: Vec<Text<'static>>, link: Option<u64>) -> ChatEntry {
    ChatEntry::Agent(ChatAgentLine { id: 40, state: kind, text: text.into(), body, open: false, link })
}

fn agent_entries() -> Vec<ChatEntry> {
    let long: Vec<Text<'static>> = (1..=9).map(|n| Text::from(format!("line {n}"))).collect();
    vec![
        agent_line(ChatAgentKind::Started, "▶ started claude in web-shop › fix/ci-482", Vec::new(), Some(4)),
        agent_line(
            ChatAgentKind::Finished { failed: false },
            "✓ claude finished in web-shop › fix/ci-482",
            long,
            Some(4),
        ),
        agent_line(
            ChatAgentKind::Waiting,
            "! claude needs you in web-shop › fix/ci-482",
            vec![Text::from("Allow this edit?")],
            Some(4),
        ),
        agent_line(ChatAgentKind::Started, "▶ started claude in web-shop › old", Vec::new(), None),
    ]
}

#[test]
fn an_agent_the_bot_started_is_drawn_with_its_link_or_closed() {
    let rows = chat_rows(&agent_entries(), ROW_W, TONE);
    let text: Vec<String> =
        rows.iter().map(|row| row.line.spans.iter().map(|span| span.content.as_ref()).collect()).collect();
    assert!(text[0].ends_with("fix/ci-482  show ") || text[0].ends_with(" show "), "{text:?}");
    assert!(text.iter().any(|row| row.trim() == "+3 lines"), "{text:?}");
    assert!(text.iter().all(|row| !row.contains("line 7")), "{text:?}");
    assert!(text.last().is_some_and(|row| row.ends_with("closed")), "{text:?}");
    let show = rows[0].hit.as_ref().expect("a link");
    assert_eq!(show.0, RowHit::Show(4));
    assert!(rows.last().expect("a row").hit.is_none());
    insta::assert_snapshot!(drawn(chat(&agent_entries())).backend());
}

#[test]
fn a_card_lists_the_agents_the_bot_started() {
    let started = |label, value: &str, pane| CardRow {
        status: Some(Status::Working),
        pane: Some(pane),
        ..CardRow::new(label, value)
    };
    let card = BotCard {
        name: "prs".into(),
        rows: vec![
            CardRow { folder: true, ..CardRow::new("folder", "~/bots/prs") },
            CardRow::new("agent", "Claude Code"),
            started("started", "claude · web-shop › fix/ci", 4),
            started("", "claude · api", 7),
        ],
    };
    let v = View { bot_card: Some(card.clone()), has_project: true, ..view(&["cornercase"]) };
    insta::assert_snapshot!(render(&v).backend());
    let a = areas();
    let at = card_row_at(a.workspaces_list, &card, Position::new(a.workspaces_list.x + 3, a.workspaces_list.y + 3));
    assert_eq!(at, Some(2));
}

#[test]
fn a_long_notice_wraps_and_keeps_its_reason() {
    let text = "no longer following claude in web-shop › fix/ci-482: a bot follows at most 20 agents";
    let rows = chat_rows(&[ChatEntry::Notice(text.into())], 40, TONE);
    let joined: String =
        rows.iter().map(|row| row.line.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect();
    assert!(rows.len() > 1 && joined.contains("20 agents"), "{joined}");
}

#[test]
fn an_ask_waiting_has_a_give_button_after_its_text() {
    let rows = chat_rows(&[ChatEntry::Asking { text: "asks for a secret: pw".into(), ask: 4 }], ROW_W, TONE);
    let (hit, cols) = rows[0].hit.clone().expect("a button");
    assert_eq!(hit, RowHit::Give(4));
    let text: String = rows[0].line.spans.iter().map(|span| span.content.as_ref()).collect();
    let start = text.find(" give… ").expect("the button");
    assert_eq!(usize::from(cols.start), text[..start].chars().count());
    assert_eq!(usize::from(cols.end - cols.start), " give… ".chars().count());
    assert!(text.contains("── asks for a secret: pw ──"), "{text}");
}
