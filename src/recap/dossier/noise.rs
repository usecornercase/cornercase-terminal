const FILLER: [&str; 20] = [
    "yes",
    "y",
    "no",
    "n",
    "ok",
    "okay",
    "go",
    "go on",
    "continue",
    "proceed",
    "thanks",
    "thank you",
    "si",
    "sí",
    "vale",
    "dale",
    "1",
    "2",
    "3",
    "sure",
];
const PASTE_CHARS: usize = 2000;
const PASTE_LINES: usize = 20;

fn plain(text: &str) -> String {
    let kept: String = text.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

pub(super) fn is_noise(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || text.starts_with('/') || text.starts_with('!') || FILLER.contains(&plain(text).as_str())
}

pub(super) fn fold_paste(text: &str) -> String {
    let text = text.trim();
    let lines = text.lines().count();
    if text.chars().count() <= PASTE_CHARS && lines <= PASTE_LINES {
        return text.to_string();
    }
    let first = text.lines().find(|line| !line.trim().is_empty()).unwrap_or_default().trim();
    format!("{first} (pasted {lines} lines)")
}

pub(super) fn trim_to(text: &str, limit: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= limit {
        return flat;
    }
    let cut: String = flat.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests;
