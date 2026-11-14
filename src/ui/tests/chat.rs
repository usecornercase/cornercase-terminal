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
fn the_empty_state_says_what_a_bot_is_and_offers_three_examples() {
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
    assert_eq!(hit, Some(ChatHit::Row(RowHit::Example(2))));
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
