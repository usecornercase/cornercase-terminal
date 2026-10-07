use std::path::Path;
use std::sync::{Arc, OnceLock};

use arborium::{GrammarStore, Highlighter};
use arborium_highlight::{FlatToken, spans_to_flat_tokens};
use ratatui::style::{Color, Modifier, Style};

pub type Segments = Vec<(String, Style)>;

pub const MAX_BYTES: usize = 2 << 20;
const TAB: &str = "    ";
const PLAIN: Style = Style::new();
const NAMES: [(&str, &str); 24] = [
    ("Dockerfile", "dockerfile"),
    ("Containerfile", "dockerfile"),
    ("Makefile", "make"),
    ("makefile", "make"),
    ("GNUmakefile", "make"),
    ("CMakeLists.txt", "cmake"),
    ("justfile", "just"),
    ("Justfile", "just"),
    (".justfile", "just"),
    (".bashrc", "bash"),
    (".bash_profile", "bash"),
    (".profile", "bash"),
    (".envrc", "bash"),
    (".env", "bash"),
    (".zshrc", "zsh"),
    (".zprofile", "zsh"),
    (".zshenv", "zsh"),
    ("Cargo.lock", "toml"),
    ("BUILD", "starlark"),
    ("BUILD.bazel", "starlark"),
    ("WORKSPACE", "starlark"),
    (".gitattributes", "gitattributes"),
    (".editorconfig", "ini"),
    ("Jenkinsfile", "javascript"),
];
const EXTENSIONS: [(&str, &str); 7] = [
    ("js", "tsx"),
    ("jsx", "tsx"),
    ("mjs", "tsx"),
    ("cjs", "tsx"),
    ("mts", "typescript"),
    ("cts", "typescript"),
    ("h", "c"),
];
const FAMILIES: [(&str, &str); 53] = [
    ("kt", "javascript"),
    ("kts", "javascript"),
    ("swift", "javascript"),
    ("cs", "javascript"),
    ("cpp", "javascript"),
    ("cc", "javascript"),
    ("cxx", "javascript"),
    ("hpp", "javascript"),
    ("hh", "javascript"),
    ("hxx", "javascript"),
    ("m", "javascript"),
    ("mm", "javascript"),
    ("scala", "javascript"),
    ("sc", "javascript"),
    ("sbt", "javascript"),
    ("dart", "javascript"),
    ("groovy", "javascript"),
    ("gradle", "javascript"),
    ("php", "javascript"),
    ("sol", "javascript"),
    ("zig", "javascript"),
    ("d", "javascript"),
    ("fs", "javascript"),
    ("fsx", "javascript"),
    ("hx", "javascript"),
    ("vala", "javascript"),
    ("glsl", "javascript"),
    ("hlsl", "javascript"),
    ("wgsl", "javascript"),
    ("res", "javascript"),
    ("prisma", "javascript"),
    ("odin", "javascript"),
    ("rb", "python"),
    ("rake", "python"),
    ("gemspec", "python"),
    ("cr", "python"),
    ("ex", "python"),
    ("exs", "python"),
    ("pl", "python"),
    ("pm", "python"),
    ("r", "python"),
    ("jl", "python"),
    ("nim", "python"),
    ("ps1", "python"),
    ("psm1", "python"),
    ("tcl", "python"),
    ("sql", "lua"),
    ("hs", "lua"),
    ("ada", "lua"),
    ("adb", "lua"),
    ("lisp", "scheme"),
    ("cl", "scheme"),
    ("rkt", "scheme"),
];
const SCRIPTS: [(&str, &str); 10] = [
    ("sh", "bash"),
    ("bash", "bash"),
    ("dash", "bash"),
    ("zsh", "zsh"),
    ("fish", "fish"),
    ("python", "python"),
    ("node", "tsx"),
    ("deno", "tsx"),
    ("bun", "tsx"),
    ("ruby", "python"),
];

fn store() -> &'static Arc<GrammarStore> {
    static STORE: OnceLock<Arc<GrammarStore>> = OnceLock::new();
    STORE.get_or_init(|| Arc::new(GrammarStore::new()))
}

fn lookup(table: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, language)| *language)
}

fn shebang(first: &str) -> Option<&'static str> {
    let command = first.strip_prefix("#!")?;
    let mut words = command.split_whitespace();
    let mut program = words.next()?.rsplit('/').next()?;
    if program == "env" {
        program = words.find(|w| !w.starts_with('-'))?;
    }
    let program = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    lookup(&SCRIPTS, program)
}

pub fn language(path: &Path, first_line: &str) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?;
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let found = lookup(&NAMES, name)
        .or_else(|| extension.as_deref().and_then(|e| lookup(&EXTENSIONS, e)))
        .or_else(|| arborium::detect_language(name))
        .or_else(|| shebang(first_line));
    found
        .filter(|language| arborium::get_language(language).is_some())
        .or_else(|| extension.as_deref().and_then(|e| lookup(&FAMILIES, e)))
}

fn style(tag: &str) -> Style {
    let fg = |colour| Style::new().fg(colour);
    match tag {
        "k" | "l" => fg(Color::Magenta),
        "f" | "m" => fg(Color::Blue),
        "s" | "tl" | "da" => fg(Color::Green),
        "c" => fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        "t" | "cr" | "ns" => fg(Color::Cyan),
        "co" | "n" | "at" => fg(Color::Yellow),
        "pr" | "tg" | "dd" | "er" => fg(Color::Red),
        "tt" => fg(Color::Blue).add_modifier(Modifier::BOLD),
        "tu" => fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
        "st" => PLAIN.add_modifier(Modifier::BOLD),
        "em" => PLAIN.add_modifier(Modifier::ITALIC),
        "tx" => PLAIN.add_modifier(Modifier::CROSSED_OUT),
        _ => PLAIN,
    }
}

pub fn lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source.split_inclusive('\n').scan(0, |offset, raw| {
        let start = *offset;
        *offset += raw.len();
        Some((start, raw.trim_end_matches(['\n', '\r'])))
    })
}

pub fn expand(text: &str) -> String {
    text.replace('\t', TAB)
}

fn push(segments: &mut Segments, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    match segments.last_mut() {
        Some((last, s)) if *s == style => last.push_str(&expand(text)),
        _ => segments.push((expand(text), style)),
    }
}

fn split(source: &str, tokens: &[FlatToken]) -> Vec<Segments> {
    let piece = |a: usize, b: usize| source.get(a..b).unwrap_or("");
    let mut first = 0;
    lines(source)
        .map(|(start, line)| {
            let end = start + line.len();
            while tokens.get(first).is_some_and(|t| t.end as usize <= start) {
                first += 1;
            }
            let mut segments = Segments::new();
            let mut pos = start;
            for token in tokens[first..].iter().take_while(|t| (t.start as usize) < end) {
                let from = (token.start as usize).max(pos);
                let to = (token.end as usize).min(end);
                push(&mut segments, piece(pos, from), PLAIN);
                if to > from {
                    push(&mut segments, piece(from, to), style(token.tag));
                    pos = to;
                }
            }
            push(&mut segments, piece(pos, end), PLAIN);
            segments
        })
        .collect()
}

pub fn highlight(source: &str, language: &str) -> Option<Vec<Segments>> {
    if source.len() > MAX_BYTES {
        return None;
    }
    let mut highlighter = Highlighter::with_store(Arc::clone(store()));
    let spans = highlighter.highlight_spans(language, source).ok()?;
    Some(split(source, &spans_to_flat_tokens(source, spans)))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn style_of(source: &str, language: &str, word: &str) -> Style {
        highlight(source, language)
            .expect("highlighted")
            .iter()
            .flatten()
            .find(|(text, _)| text == word || text.trim() == word)
            .map_or_else(|| panic!("{word:?} is a segment of its own"), |(_, style)| *style)
    }

    #[rstest]
    #[case::rust_keyword("fn main() {}\n", "rust", "fn", Color::Magenta)]
    #[case::rust_function("fn main() {}\n", "rust", "main", Color::Blue)]
    #[case::typescript_type("let a: Order = load();\n", "typescript", "Order", Color::Cyan)]
    #[case::typescript_string("const a = 'x';\n", "typescript", "'x'", Color::Green)]
    #[case::python_number("x = 42\n", "python", "42", Color::Yellow)]
    #[case::jsx_tag("const a = <div className=\"x\" />;\n", "tsx", "div", Color::Red)]
    #[case::jsx_attribute("const a = <div className=\"x\" />;\n", "tsx", "className", Color::Yellow)]
    fn colours_tokens_with_the_terminal_palette(
        #[case] source: &str,
        #[case] language: &str,
        #[case] word: &str,
        #[case] colour: Color,
    ) {
        assert_eq!(style_of(source, language, word).fg, Some(colour));
    }

    #[test]
    fn comments_are_grey_and_italic() {
        let style = style_of("# a note\nx = 1\n", "python", "# a note");
        assert_eq!((style.fg, style.add_modifier.contains(Modifier::ITALIC)), (Some(Color::DarkGray), true));
    }

    #[test]
    fn keeps_one_entry_per_line_with_tabs_expanded() {
        let lines = highlight("fn a() {\n\tb();\n}\n", "rust").expect("highlighted");
        let text: Vec<String> = lines.iter().map(|l| l.iter().map(|(t, _)| t.as_str()).collect()).collect();
        assert_eq!(text, ["fn a() {", "    b();", "}"]);
    }

    #[test]
    fn a_multiline_comment_colours_every_line() {
        let lines = highlight("/* one\ntwo */\nfn a() {}\n", "rust").expect("highlighted");
        assert_eq!(lines[1].first().map(|(t, s)| (t.as_str(), s.fg)), Some(("two */", Some(Color::DarkGray))));
    }

    #[test]
    fn a_huge_source_is_left_plain() {
        assert_eq!(highlight(&"x".repeat(MAX_BYTES + 1), "rust"), None);
    }

    #[rstest]
    #[case::by_extension("src/main.rs", "", Some("rust"))]
    #[case::javascript_reads_jsx("App.jsx", "", Some("tsx"))]
    #[case::plain_javascript_too("index.js", "", Some("tsx"))]
    #[case::header_as_c("lib.h", "", Some("c"))]
    #[case::by_name("Dockerfile", "", Some("dockerfile"))]
    #[case::dotfile("home/.zshrc", "", Some("zsh"))]
    #[case::shebang("bin/deploy", "#!/usr/bin/env bash", Some("bash"))]
    #[case::versioned_shebang("bin/tool", "#!/usr/bin/python3", Some("python"))]
    #[case::family_with_slashes("App.kt", "", Some("javascript"))]
    #[case::family_with_hashes("app/models/user.rb", "", Some("python"))]
    #[case::family_with_dashes("db/schema.sql", "", Some("lua"))]
    #[case::text_stays_plain("notes.txt", "", None)]
    #[case::unknown("data.xyz", "", None)]
    fn picks_a_grammar_for_every_file(#[case] path: &str, #[case] first: &str, #[case] expected: Option<&str>) {
        assert_eq!(language(Path::new(path), first), expected);
    }
}
