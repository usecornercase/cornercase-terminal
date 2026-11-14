use crate::bots::chat::{Kind, Line};
use crate::text;

const WIDTH: usize = 120;

fn first(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default();
    text::truncate_right(line, WIDTH)
}

fn line_text(line: &Line) -> String {
    match line.kind {
        Kind::You => format!("you: {}", first(&line.text)),
        Kind::Step => format!("▸ {}", first(&line.text)),
        Kind::Bot => format!("answered: {}", first(&line.text)),
        Kind::Error => format!("error: {}", first(&line.text)),
        Kind::Notice => format!("notice: {}", first(&line.text)),
    }
}

pub(super) fn event_text(bot: &str, news: (Option<&Line>, Option<&str>, Option<&str>)) -> String {
    let (line, state, change) = news;
    let what = line.map(line_text).or_else(|| state.or(change).map(str::to_string)).unwrap_or_default();
    format!("bot {bot}  {what}")
}
