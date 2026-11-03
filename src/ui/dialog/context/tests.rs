use rstest::rstest;

use super::*;

fn slice(label: &str, tokens: u64, fill: Fill) -> Slice {
    Slice { label: label.into(), tokens, text: format!("{tokens}"), share: "0.0%".into(), fill }
}

fn view(slices: Vec<Slice>, max: u64) -> ContextView {
    ContextView { max, slices, ..ContextView::default() }
}

fn bar_text(view: &ContextView, width: usize) -> String {
    bar(view, width, Style::default()).spans.iter().map(|s| s.content.as_ref()).collect()
}

#[rstest]
#[case::wide(40)]
#[case::narrow(7)]
#[case::one(1)]
fn the_bar_always_fills_its_width(#[case] width: usize) {
    let view = view(
        vec![
            slice("a", 3, Fill::Used(0)),
            slice("b", 1, Fill::Used(1)),
            slice("free", 5, Fill::Free),
            slice("buffer", 1, Fill::Buffer),
        ],
        10,
    );
    assert_eq!(bar_text(&view, width).chars().count(), width);
}

#[test]
fn the_bar_runs_from_the_used_slices_through_free_to_the_buffer() {
    let view = view(
        vec![
            slice("a", 2, Fill::Used(0)),
            slice("b", 2, Fill::Used(1)),
            slice("free", 5, Fill::Free),
            slice("buffer", 1, Fill::Buffer),
        ],
        10,
    );
    assert_eq!(bar_text(&view, 10), "■■■■□□□□□▒");
}

#[test]
fn a_tiny_slice_may_get_no_cell() {
    let view = view(vec![slice("a", 1, Fill::Used(0)), slice("free", 999, Fill::Free)], 1_000);
    assert_eq!(bar_text(&view, 10), "□□□□□□□□□□");
}

#[test]
fn rows_cut_the_path_at_the_start_and_keep_the_tokens() {
    let row = Row {
        left: "~/code/web-shop/docs/very/long/AGENTS.md".into(),
        middle: String::new(),
        right: "8.5k".into(),
        muted: false,
        path: true,
    };
    let line = row_line(&row, 24, Style::default());
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text.chars().count(), 24, "{text}");
    assert!(text.starts_with("  …") && text.ends_with("8.5k"), "{text}");
}

#[test]
fn the_notes_have_a_bar_rows_sections_and_a_notice() {
    let mut full = view(vec![slice("Prompt", 2, Fill::Used(0)), slice("Free space", 8, Fill::Free)], 10);
    full.not_loaded = Some("Not loaded: MCP tools 1k".into());
    full.sections = vec![Section {
        title: "Skills",
        rows: vec![Row { left: "34".into(), middle: String::new(), right: "6k".into(), muted: false, path: false }],
    }];
    full.notice = Some("No conversation yet".into());

    let lines: Vec<String> = context_notes(&full, 30, Color::Gray)
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    assert_eq!(lines.len(), 10, "{lines:#?}");
    assert!(lines[2].starts_with("■ Prompt") && lines[3].starts_with("□ Free space"), "{lines:#?}");
    assert_eq!(lines[4], "Not loaded: MCP tools 1k");
    assert_eq!(lines[6], "Skills");
    assert_eq!(lines[9], "No conversation yet");
}

#[test]
fn nothing_to_show_draws_nothing() {
    assert_eq!(context_notes(&ContextView::default(), 30, Color::Gray).len(), 0);
}
