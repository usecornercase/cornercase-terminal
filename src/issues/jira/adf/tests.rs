use rstest::rstest;
use serde_json::json;

use super::*;

fn with_content(mut node: Value, content: Value) -> Value {
    node["content"] = content;
    node
}

fn doc(content: Value) -> Value {
    with_content(json!({ "type": "doc", "version": 1 }), content)
}

fn text_node(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

fn marked_node(text: &str, marks: Value) -> Value {
    let mut node = text_node(text);
    node["marks"] = marks;
    node
}

fn paragraph(content: Value) -> Value {
    with_content(json!({ "type": "paragraph" }), content)
}

fn item(content: Value) -> Value {
    with_content(json!({ "type": "listItem" }), content)
}

#[rstest]
#[case::null(Value::Null, "")]
#[case::plain_text(json!("already text"), "already text")]
#[case::empty_doc(doc(json!([])), "")]
fn takes_what_is_not_a_document_as_it_is(#[case] value: Value, #[case] expected: &str) {
    assert_eq!(markdown(&value), expected);
}

#[test]
fn paragraphs_are_separated_by_a_blank_line() {
    let value = doc(json!([paragraph(json!([text_node("one")])), paragraph(json!([text_node("two")]))]));
    assert_eq!(markdown(&value), "one\n\ntwo");
}

#[test]
fn headings_keep_their_level() {
    let value = doc(json!([{ "type": "heading", "attrs": { "level": 3 }, "content": [text_node("Steps")] }]));
    assert_eq!(markdown(&value), "### Steps");
}

#[rstest]
#[case::strong(json!([{ "type": "strong" }]), "**bold**")]
#[case::em(json!([{ "type": "em" }]), "*bold*")]
#[case::strike(json!([{ "type": "strike" }]), "~~bold~~")]
#[case::code(json!([{ "type": "code" }]), "`bold`")]
#[case::link(json!([{ "type": "link", "attrs": { "href": "https://x.dev/a b" } }]), "[bold](https://x.dev/a%20b)")]
#[case::two(json!([{ "type": "strong" }, { "type": "em" }]), "***bold***")]
#[case::unknown(json!([{ "type": "textColor", "attrs": { "color": "#f00" } }]), "bold")]
fn marks_become_markdown(#[case] marks: Value, #[case] expected: &str) {
    assert_eq!(markdown(&doc(json!([paragraph(json!([marked_node("bold", marks)]))]))), expected);
}

#[test]
fn marks_leave_the_spaces_around_the_text_outside() {
    let value = doc(json!([paragraph(json!([
        text_node("a"),
        marked_node(" b ", json!([{ "type": "strong" }])),
        text_node("c")
    ]))]));
    assert_eq!(markdown(&value), "a **b** c");
}

#[rstest]
#[case::stars("2 * 3 * 4", "2 \\* 3 \\* 4")]
#[case::brackets("[not a link]", "\\[not a link\\]")]
#[case::html("<div>", "\\<div>")]
#[case::snake_case_stays("snake_case_name", "snake_case_name")]
#[case::emphasis_underscores("_almost_", "\\_almost\\_")]
fn plain_text_is_escaped(#[case] text: &str, #[case] expected: &str) {
    assert_eq!(markdown(&doc(json!([paragraph(json!([text_node(text)]))]))), expected);
}

#[test]
fn code_spans_grow_around_backticks() {
    let value = doc(json!([paragraph(json!([marked_node("a`b", json!([{ "type": "code" }]))]))]));
    assert_eq!(markdown(&value), "``a`b``");
}

#[test]
fn a_hard_break_ends_the_line() {
    let value = doc(json!([paragraph(json!([text_node("a"), { "type": "hardBreak" }, text_node("b")]))]));
    assert_eq!(markdown(&value), "a  \nb");
}

#[test]
fn bullet_lists_nest() {
    let nested =
        json!({ "type": "bulletList", "content": [item(json!([paragraph(json!([text_node("inner")]))]))] });
    let value = doc(json!([{ "type": "bulletList", "content": [
        item(json!([paragraph(json!([text_node("outer")])), nested])),
        item(json!([paragraph(json!([text_node("next")]))]))
    ] }]));
    assert_eq!(markdown(&value), "- outer\n  - inner\n- next");
}

#[test]
fn ordered_lists_count_from_their_start() {
    let value = doc(json!([{ "type": "orderedList", "attrs": { "order": 3 }, "content": [
        item(json!([paragraph(json!([text_node("three")]))])),
        item(json!([paragraph(json!([text_node("four")]))]))
    ] }]));
    assert_eq!(markdown(&value), "3. three\n4. four");
}

#[test]
fn task_lists_become_checkboxes() {
    let value = doc(json!([{ "type": "taskList", "attrs": { "localId": "l" }, "content": [
        { "type": "taskItem", "attrs": { "state": "DONE" }, "content": [text_node("Reproduce")] },
        { "type": "taskItem", "attrs": { "state": "TODO" }, "content": [text_node("Fix")] }
    ] }]));
    assert_eq!(markdown(&value), "- [x] Reproduce\n- [ ] Fix");
}

#[test]
fn code_blocks_keep_their_language() {
    let value = doc(json!([{ "type": "codeBlock", "attrs": { "language": "rust" }, "content": [
        text_node("fn main() {}\n")
    ] }]));
    assert_eq!(markdown(&value), "```rust\nfn main() {}\n```");
}

#[test]
fn a_code_block_holding_a_fence_gets_a_longer_one() {
    let value = doc(json!([{ "type": "codeBlock", "content": [text_node("```")] }]));
    assert_eq!(markdown(&value), "````\n```\n````");
}

#[test]
fn quotes_and_panels_are_quoted() {
    let value = doc(json!([
        { "type": "blockquote", "content": [paragraph(json!([text_node("a")])), paragraph(json!([text_node("b")]))] },
        { "type": "panel", "attrs": { "panelType": "info" }, "content": [paragraph(json!([text_node("c")]))] }
    ]));
    assert_eq!(markdown(&value), "> a\n>\n> b\n\n> c");
}

#[test]
fn a_rule_is_a_thematic_break() {
    let value = doc(json!([paragraph(json!([text_node("a")])), { "type": "rule" }]));
    assert_eq!(markdown(&value), "a\n\n---");
}

#[test]
fn tables_become_pipe_tables() {
    let cell = |kind: &str, text: &str| json!({ "type": kind, "content": [paragraph(json!([text_node(text)]))] });
    let value = doc(json!([{ "type": "table", "content": [
        { "type": "tableRow", "content": [cell("tableHeader", "Key"), cell("tableHeader", "Value")] },
        { "type": "tableRow", "content": [cell("tableCell", "a|b"), cell("tableCell", "1")] },
        { "type": "tableRow", "content": [cell("tableCell", "short")] }
    ] }]));
    assert_eq!(markdown(&value), "| Key | Value |\n| --- | --- |\n| a\\|b | 1 |\n| short |  |");
}

#[rstest]
#[case::mention(json!({ "type": "mention", "attrs": { "id": "1", "text": "@Ana" } }), "@Ana")]
#[case::mention_without_at(json!({ "type": "mention", "attrs": { "id": "1", "text": "Ana" } }), "@Ana")]
#[case::emoji(json!({ "type": "emoji", "attrs": { "shortName": ":smile:", "text": "😄" } }), "😄")]
#[case::emoji_name(json!({ "type": "emoji", "attrs": { "shortName": ":party:" } }), ":party:")]
#[case::card(json!({ "type": "inlineCard", "attrs": { "url": "https://x.dev" } }), "<https://x.dev>")]
#[case::date(json!({ "type": "date", "attrs": { "timestamp": "1759708800000" } }), "2025-10-06")]
#[case::status(json!({ "type": "status", "attrs": { "text": "IN REVIEW", "color": "blue" } }), "`IN REVIEW`")]
#[case::media(json!({ "type": "mediaInline", "attrs": { "id": "x" } }), "")]
fn inline_nodes_become_text(#[case] node: Value, #[case] expected: &str) {
    assert_eq!(markdown(&doc(json!([paragraph(json!([node]))]))), expected);
}

#[test]
fn an_expand_shows_its_title() {
    let value = doc(json!([{ "type": "expand", "attrs": { "title": "Logs" }, "content": [
        paragraph(json!([text_node("trace")]))
    ] }]));
    assert_eq!(markdown(&value), "**Logs**\n\ntrace");
}

#[test]
fn unknown_nodes_show_their_text() {
    let value = doc(json!([
        { "type": "layoutSection", "content": [{ "type": "layoutColumn", "content": [paragraph(json!([text_node("left")]))] }] },
        { "type": "mediaSingle", "content": [{ "type": "media", "attrs": { "id": "x" } }] },
        { "type": "somethingNew", "content": [text_node("inline")] }
    ]));
    assert_eq!(markdown(&value), "left\n\ninline");
}
