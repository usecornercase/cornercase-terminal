use std::path::Path;
use std::sync::{Arc, OnceLock};

use arborium::{GrammarStore, Highlighter};
use arborium_highlight::{FlatToken, spans_to_flat_tokens};
use ratatui::style::{Color, Modifier, Style};

pub(crate) type Segments = Vec<(String, Style)>;

pub(crate) const MAX_BYTES: usize = 2 << 20;
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

pub(crate) fn language(path: &Path, first_line: &str) -> Option<&'static str> {
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

pub(crate) fn lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source.split_inclusive('\n').scan(0, |offset, raw| {
        let start = *offset;
        *offset += raw.len();
        Some((start, raw.trim_end_matches(['\n', '\r'])))
    })
}

pub(crate) fn expand(text: &str) -> String {
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

pub(crate) fn highlight(source: &str, language: &str) -> Option<Vec<Segments>> {
    if source.len() > MAX_BYTES {
        return None;
    }
    let mut highlighter = Highlighter::with_store(Arc::clone(store()));
    let spans = highlighter.highlight_spans(language, source).ok()?;
    Some(split(source, &spans_to_flat_tokens(source, spans)))
}

#[cfg(test)]
mod tests;
