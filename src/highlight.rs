pub(crate) mod markdown;
pub(crate) mod syntax;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use ratatui::style::{Color, Modifier, Style};
use syntect::easy::ScopeRangeIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

pub(crate) type Highlighted = Arc<Vec<Vec<(String, Style)>>>;

const CACHE_SIZE: usize = 256;
const MAX_CODE: usize = 50_000;
const PLAIN: Style = Style::new();
const RULES: [(&str, Style); 22] = [
    ("comment", Style::new().fg(Color::DarkGray)),
    ("keyword.operator", PLAIN),
    ("keyword", Style::new().fg(Color::Magenta)),
    ("storage", Style::new().fg(Color::Magenta)),
    ("constant.language", Style::new().fg(Color::Magenta)),
    ("string", Style::new().fg(Color::Green)),
    ("constant.character", Style::new().fg(Color::Green)),
    ("markup.inserted", Style::new().fg(Color::Green)),
    ("constant.numeric", Style::new().fg(Color::Yellow)),
    ("constant.other.symbol", Style::new().fg(Color::Yellow)),
    ("markup.list", Style::new().fg(Color::Yellow)),
    ("constant.other", PLAIN),
    ("entity.name", Style::new().fg(Color::Cyan)),
    ("entity.other.attribute-name", Style::new().fg(Color::Cyan)),
    ("entity.other.inherited-class", Style::new().fg(Color::Cyan)),
    ("support.function", Style::new().fg(Color::Cyan)),
    ("support.type", Style::new().fg(Color::Cyan)),
    ("support.class", Style::new().fg(Color::Cyan)),
    ("markup.deleted", Style::new().fg(Color::Red)),
    ("markup.heading", Style::new().add_modifier(Modifier::BOLD)),
    ("markup.bold", Style::new().add_modifier(Modifier::BOLD)),
    ("markup.italic", Style::new().add_modifier(Modifier::ITALIC)),
];
const ALIASES: [(&str, &str); 15] = [
    ("ts", "js"),
    ("tsx", "js"),
    ("typescript", "js"),
    ("jsx", "js"),
    ("mjs", "js"),
    ("cjs", "js"),
    ("javascript", "js"),
    ("shell", "bash"),
    ("console", "bash"),
    ("zsh", "bash"),
    ("jsonc", "json"),
    ("json5", "json"),
    ("vue", "html"),
    ("svelte", "html"),
    ("golang", "go"),
];

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn rules() -> &'static [(Scope, Style)] {
    static PARSED: OnceLock<Vec<(Scope, Style)>> = OnceLock::new();
    PARSED.get_or_init(|| RULES.iter().filter_map(|(name, style)| Scope::new(name).ok().map(|s| (s, *style))).collect())
}

fn cache() -> &'static Mutex<HashMap<u64, Highlighted>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Highlighted>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn language_token(info: &str) -> Option<String> {
    let token = info.split(|c: char| c.is_whitespace() || c == ',' || c == '{').next()?.trim().to_lowercase();
    if token.is_empty() {
        return None;
    }
    Some(ALIASES.iter().find(|(alias, _)| *alias == token).map_or(token, |(_, to)| (*to).to_string()))
}

fn syntax(token: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    set.find_syntax_by_token(token).or_else(|| set.syntaxes().iter().find(|s| s.name.eq_ignore_ascii_case(token)))
}

pub(crate) fn highlight(code: &str, info: &str) -> Option<Highlighted> {
    let token = language_token(info)?;
    if code.len() > MAX_CODE {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    (&token, code).hash(&mut hasher);
    let key = hasher.finish();
    if let Some(found) = cache().lock().get(&key) {
        return Some(Arc::clone(found));
    }
    let highlighted = Arc::new(parse(code, syntax(&token)?)?);
    let mut cache = cache().lock();
    if cache.len() >= CACHE_SIZE {
        cache.clear();
    }
    cache.insert(key, Arc::clone(&highlighted));
    Some(highlighted)
}

fn parse(code: &str, syntax: &SyntaxReference) -> Option<Vec<Vec<(String, Style)>>> {
    let set = syntaxes();
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ops = state.parse_line(line, set).ok()?;
        let mut spans: Vec<(String, Style)> = Vec::new();
        for (range, op) in ScopeRangeIterator::new(&ops, line) {
            stack.apply(op).ok()?;
            let text = line[range].trim_end_matches(['\n', '\r']).replace('\t', "    ");
            if text.is_empty() {
                continue;
            }
            let style = style_of(&stack);
            match spans.last_mut() {
                Some((last, s)) if *s == style => last.push_str(&text),
                _ => spans.push((text, style)),
            }
        }
        lines.push(spans);
    }
    Some(lines)
}

fn style_of(stack: &ScopeStack) -> Style {
    stack
        .as_slice()
        .iter()
        .rev()
        .find_map(|scope| rules().iter().find(|(rule, _)| rule.is_prefix_of(*scope)).map(|(_, style)| *style))
        .unwrap_or(PLAIN)
}

#[cfg(test)]
mod tests;
