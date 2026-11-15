use ratatui::text::Line as Text;

use super::*;
use crate::editor::Editor;

const TONE: Tone = Tone { muted: Color::DarkGray, surface: DARK_SURFACE };
const CHAT_W: u16 = 80;
const CHAT_H: u16 = 24;

fn markdown(source: &str) -> Vec<Text<'static>> {
    crate::highlight::markdown::render(source, usize::from(CHAT_W) - 2)
}

fn you(text: &str, stamp: &str) -> ChatEntry {
    ChatEntry::You { text: text.into(), stamp: stamp.into() }
}

fn bot(text: &str) -> ChatEntry {
    ChatEntry::Bot { head: "prs · 09:12".into(), body: markdown(text) }
}

fn steps(texts: &[&str], open: bool) -> ChatEntry {
    ChatEntry::Steps { first: 1, texts: texts.iter().map(ToString::to_string).collect(), open }
}

fn chat(entries: &[ChatEntry]) -> ChatView {
    ChatView {
        name: "prs".into(),
        detail: "claude · opus · high · home".into(),
        rows: chat_rows(entries, CHAT_W - 2, TONE),
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
    let view = View { chat: Some(chat), ..view(&[]) };
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
    let rows = chat_rows(&[steps(&refs, true)], CHAT_W - 2, TONE);
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
    view.rows = empty_rows(CHAT_W - 2, TONE);
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
    let rows = chat_rows(&[you(&long, "09:12")], CHAT_W - 2, TONE);
    let widest = rows.iter().map(|row| row.line.width()).max().unwrap_or_default();
    assert!(widest <= usize::from(CHAT_W - 2));
    assert!(rows.iter().all(|row| row.line.spans[0].content.len() >= 12));
}

#[test]
fn a_link_in_an_answer_is_found_under_a_click() {
    let rows = chat_rows(&[bot("see https://example.com/a for it")], CHAT_W - 2, TONE);
    let row = rows.iter().find(|row| row.line.to_string().contains("example")).expect("the row");
    let col = row.line.to_string().find("example").expect("a column");
    assert_eq!(bots::chat::link_at(row, col).as_deref(), Some("https://example.com/a"));
    assert_eq!(bots::chat::link_at(row, 0), None);
}

#[test]
fn the_retry_button_is_only_on_the_error_row() {
    let rows = chat_rows(&[ChatEntry::Error { id: 3, text: "boom".into(), retry: true }], CHAT_W - 2, TONE);
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
    view.rows = empty_rows(CHAT_W - 2, TONE);
    view.empty = true;
    let areas = chat_areas(pane, 1, 1);
    let top = row_y(areas.lines, view.rows.len(), true, view.rows.len() - 1);
    let hit = chat_hit(&areas, &view, Position::new(areas.lines.x + 30, top));
    assert_eq!(hit, Some(ChatHit::Row(RowHit::Example(3))));
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
        answer: Some(("prs · 09:05".into(), markdown("Two are **blocked**:\n\n- #482 waits for a review"))),
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
    let rows = entry_rows(&ChatEntry::Run(RunBlock { open: true, ..run_block() }), CHAT_W - 2, TONE);
    let text: Vec<String> =
        rows.iter().map(|row| row.line.spans.iter().map(|s| s.content.as_ref()).collect()).collect();
    assert!(text.iter().any(|row| row.starts_with("▸ gh pr list")), "{text:?}");
}

#[test]
fn a_failed_run_offers_run_again() {
    let block = RunBlock { answer: None, error: Some("claude is not installed".into()), again: true, ..run_block() };
    let rows = entry_rows(&ChatEntry::Run(block), CHAT_W - 2, TONE);
    let last = rows.last().expect("a row");
    let text: String = last.line.spans.iter().map(|span| span.content.as_ref()).collect();
    assert!(text.ends_with(" run again "), "{text}");
    assert_eq!(last.hit.as_ref().map(|(hit, _)| hit.clone()), Some(RowHit::RunAgain(12)));
    let without = RunBlock { again: false, ..run_block() };
    let rows =
        entry_rows(&ChatEntry::Run(RunBlock { answer: None, error: Some("x".into()), ..without }), CHAT_W - 2, TONE);
    assert!(rows.iter().all(|row| row.hit.is_none() || matches!(row.hit, Some((RowHit::Unfold(_), _)))));
}

#[test]
fn a_run_that_is_answering_says_so() {
    let block = RunBlock { answer: None, answering: true, ..run_block() };
    let rows = entry_rows(&ChatEntry::Run(block), CHAT_W - 2, TONE);
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
