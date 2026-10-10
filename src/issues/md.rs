pub fn item_lines(marker: &str, body: &str) -> String {
    let pad = " ".repeat(marker.chars().count());
    let mut lines = body.lines();
    let first = format!("{marker}{}", lines.next().unwrap_or_default());
    std::iter::once(first)
        .chain(lines.map(|line| if line.is_empty() { String::new() } else { format!("{pad}{line}") }))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

pub fn prefix_lines(prefix: &str, empty: &str, text: &str) -> String {
    text.lines()
        .map(|line| if line.is_empty() { empty.to_string() } else { format!("{prefix}{line}") })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn grid(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let line = |cells: &[String]| {
        let padded = (0..width).map(|i| cells.get(i).map_or("", String::as_str));
        format!("| {} |", padded.collect::<Vec<_>>().join(" | "))
    };
    let mut lines = vec![line(&rows[0]), line(&vec!["---".to_string(); width])];
    lines.extend(rows[1..].iter().map(|row| line(row)));
    lines.join("\n")
}

pub fn one_line(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

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
