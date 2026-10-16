use rstest::rstest;

use super::*;

#[rstest]
#[case::strong("<p>Fix the <strong>login</strong> flow</p>", "Fix the **login** flow")]
#[case::emphasis_and_strike("<p><em>a</em> and <s>b</s></p>", "*a* and ~~b~~")]
#[case::nested_marks("<p><strong><em>x</em></strong></p>", "***x***")]
#[case::space_inside_a_mark("<p><strong>a </strong>b</p>", "**a** b")]
#[case::inline_code("<p>run <code>cargo test</code></p>", "run `cargo test`")]
#[case::link(r#"<p><a href="https://plane.so/x" target="_blank">Plane</a></p>"#, "[Plane](https://plane.so/x)")]
#[case::link_without_text(r#"<p><a href="https://plane.so/x"></a></p>"#, "<https://plane.so/x>")]
#[case::entities("<p>a &amp; b &lt;c&gt; &#65;</p>", "a & b \\<c> A")]
#[case::hard_break("<p>one<br>two</p>", "one  \ntwo")]
#[case::image(r#"<p><img src="https://x.dev/a.png" alt="shot"></p>"#, "![shot](https://x.dev/a.png)")]
#[case::unknown_tags_keep_their_text("<custom-tag>text</custom-tag>", "text")]
#[case::plain_text("just text", "just text")]
#[case::nothing("", "")]
#[case::empty_paragraph("<p></p>", "")]
#[case::stray_closing_tag("</div><p>a</p>", "a")]
fn turns_inline_html_into_markdown(#[case] html: &str, #[case] expected: &str) {
    assert_eq!(markdown(html), expected);
}

#[rstest]
#[case::heading("<h2>Plan</h2><p>Body</p>", "## Plan\n\nBody")]
#[case::paragraphs_separated_by_newlines("<p>a</p>\n<p>b</p>", "a\n\nb")]
#[case::rule("<p>a</p><hr><p>b</p>", "a\n\n---\n\nb")]
#[case::quote("<blockquote><p>quoted</p></blockquote>", "> quoted")]
#[case::code_block("<pre><code class=\"language-rust\">fn main() {}\n</code></pre>", "```rust\nfn main() {}\n```")]
#[case::code_block_with_entities("<pre><code>a &lt; b</code></pre>", "```\na < b\n```")]
fn turns_blocks_into_markdown(#[case] html: &str, #[case] expected: &str) {
    assert_eq!(markdown(html), expected);
}

#[rstest]
#[case::bullets("<ul><li><p>one</p></li><li><p>two</p></li></ul>", "- one\n- two")]
#[case::numbers_from_start(r#"<ol start="3"><li><p>a</p></li><li><p>b</p></li></ol>"#, "3. a\n4. b")]
#[case::nested("<ul><li><p>a</p><ul><li><p>b</p></li></ul></li></ul>", "- a\n  - b")]
#[case::items_without_paragraphs("<ul><li>a</li><li>b</li></ul>", "- a\n- b")]
#[case::tasks(
    concat!(
        r#"<ul data-type="taskList">"#,
        r#"<li data-checked="true"><label><input type="checkbox" checked="checked"><span></span></label>"#,
        "<div><p>done</p></div></li>",
        r#"<li data-checked="false"><label><input type="checkbox"><span></span></label>"#,
        "<div><p>todo</p></div></li></ul>"
    ),
    "- [x] done\n- [ ] todo"
)]
fn turns_lists_into_markdown(#[case] html: &str, #[case] expected: &str) {
    assert_eq!(markdown(html), expected);
}

#[test]
fn a_table_becomes_a_markdown_table() {
    let html = "<table><tbody><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2|3</td></tr></tbody></table>";
    assert_eq!(markdown(html), "| A | B |\n| --- | --- |\n| 1 | 2\\|3 |");
}

#[test]
fn attributes_may_be_quoted_bare_or_empty() {
    let attrs = attributes(r#" href="a b" data-x=1 checked  title='it"s'"#);
    let expected = [("href", "a b"), ("data-x", "1"), ("checked", ""), ("title", "it\"s")];
    assert_eq!(attrs, expected.map(|(k, v)| (k.to_string(), v.to_string())));
}
