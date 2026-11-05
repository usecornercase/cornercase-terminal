use std::iter::Peekable;
use std::str::Chars;

type Cursor<'a> = Peekable<Chars<'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) section: String,
    pub(crate) subsection: Option<String>,
    pub(crate) key: String,
    pub(crate) value: Option<String>,
}

#[derive(Default)]
struct Value {
    text: String,
    spaces: usize,
    quoted: bool,
}

impl Value {
    fn push(&mut self, c: char) {
        self.push_spaces();
        self.text.push(c);
    }

    fn toggle(&mut self) {
        self.push_spaces();
        self.quoted = !self.quoted;
    }

    fn push_spaces(&mut self) {
        self.text.extend(std::iter::repeat_n(' ', std::mem::take(&mut self.spaces)));
    }

    fn escape(&mut self, chars: &mut Cursor) -> Option<()> {
        self.push_spaces();
        let c = match chars.next().unwrap_or('\n') {
            '\n' => return Some(()),
            't' => '\t',
            'b' => '\u{8}',
            'n' => '\n',
            c @ ('\\' | '"') => c,
            _ => {
                skip_line(chars);
                return None;
            }
        };
        self.text.push(c);
        Some(())
    }
}

pub(crate) fn entries(text: &str) -> Vec<Entry> {
    let text = text.replace("\r\n", "\n");
    let mut chars = text.chars().peekable();
    let mut section = None;
    let mut entries = Vec::new();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() || c == '\u{feff}' => {}
            '[' => section = header(&mut chars),
            c if c.is_ascii_alphabetic() => entries.extend(entry(c, &mut chars, section.as_ref())),
            _ => skip_line(&mut chars),
        }
    }
    entries
}

fn header(chars: &mut Cursor) -> Option<(String, Option<String>)> {
    let name = take_while(chars, |c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.')).to_ascii_lowercase();
    skip_blanks(chars);
    let header = match chars.next() {
        Some(']') => Some(match name.split_once('.') {
            Some((section, subsection)) => (section.to_string(), Some(subsection.to_string())),
            None => (name, None),
        }),
        Some('"') => subsection(chars).filter(|_| chars.next_if_eq(&']').is_some()).map(|sub| (name, Some(sub))),
        Some('\n') | None => return None,
        Some(_) => None,
    };
    if header.is_none() {
        skip_line(chars);
    }
    header.filter(|(section, _)| !section.is_empty())
}

fn subsection(chars: &mut Cursor) -> Option<String> {
    let mut name = String::new();
    loop {
        match chars.next_if(|&c| c != '\n')? {
            '"' => return Some(name),
            '\\' => name.push(chars.next_if(|&c| c != '\n')?),
            c => name.push(c),
        }
    }
}

fn entry(first: char, chars: &mut Cursor, section: Option<&(String, Option<String>)>) -> Option<Entry> {
    let mut key = first.to_string();
    key.push_str(&take_while(chars, |c| c.is_ascii_alphanumeric() || c == '-'));
    skip_blanks(chars);
    let value = match chars.next() {
        Some('\n') | None => None,
        Some('=') => Some(value(chars)?),
        Some(_) => {
            skip_line(chars);
            return None;
        }
    };
    let (section, subsection) = section?.clone();
    Some(Entry { section, subsection, key: key.to_ascii_lowercase(), value })
}

fn value(chars: &mut Cursor) -> Option<String> {
    let mut value = Value::default();
    loop {
        match chars.next().unwrap_or('\n') {
            '\n' => return (!value.quoted).then_some(value.text),
            c if !value.quoted && c.is_whitespace() => value.spaces += usize::from(!value.text.is_empty()),
            '#' | ';' if !value.quoted => {
                skip_line(chars);
                return Some(value.text);
            }
            '\\' => value.escape(chars)?,
            '"' => value.toggle(),
            c => value.push(c),
        }
    }
}

fn take_while(chars: &mut Cursor, wanted: impl Fn(char) -> bool) -> String {
    std::iter::from_fn(|| chars.next_if(|&c| wanted(c))).collect()
}

fn skip_blanks(chars: &mut Cursor) {
    while chars.next_if(|&c| c == ' ' || c == '\t').is_some() {}
}

fn skip_line(chars: &mut Cursor) {
    chars.by_ref().find(|&c| c == '\n');
}

#[cfg(test)]
mod tests;
