pub(crate) trait Markup: Sync {
    fn escape(&self, text: &str) -> String;

    fn bold(&self, text: &str) -> String;

    fn pre(&self, text: &str, room: usize) -> String;

    fn room(&self, header: &str) -> usize;

    fn fits(&self, header: &str, part: &str) -> bool;

    fn markdown(&self, source: &str) -> Vec<String>;

    fn lines(&self, lines: &[String]) -> Vec<String>;
}

pub(crate) fn tail(text: &str, limit: usize, length: impl Fn(&str) -> usize) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0;
    for line in text.lines().rev() {
        let size = length(line) + 1;
        if used + size > limit {
            break;
        }
        used += size;
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n")
}

pub(crate) fn pack(lines: &[String], limit: usize, length: impl Fn(&str) -> usize) -> Vec<String> {
    let mut messages: Vec<(String, usize)> = Vec::new();
    for line in lines {
        let size = length(line);
        match messages.last_mut() {
            Some((message, used)) if *used + 1 + size <= limit => {
                message.push('\n');
                message.push_str(line);
                *used += 1 + size;
            }
            _ => messages.push((line.clone(), size)),
        }
    }
    messages.into_iter().map(|(message, _)| message).collect()
}
