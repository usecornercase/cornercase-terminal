pub fn code_span(text: &str) -> String {
    let ticks = "`".repeat(longest_run(text, '`') + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') { " " } else { "" };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

pub fn longest_run(text: &str, c: char) -> usize {
    let (mut longest, mut run) = (0, 0);
    for ch in text.chars() {
        run = if ch == c { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

pub fn escape(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let word = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(|c| c.is_alphanumeric());
        let inside_a_word = c == '_' && word(i.checked_sub(1)) && word(Some(i + 1));
        if "\\`*_[]<~".contains(c) && !inside_a_word {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
