use serde::{Deserialize, Serialize};

use crate::graphics::detect::{in_multiplexer, version as parse_version};

const MIN_GHOSTTY: (u32, u32, u32) = (1, 0, 0);
const MIN_KITTY: (u32, u32, u32) = (0, 31, 0);
const MIN_FOOT: (u32, u32, u32) = (1, 12, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Terminal {
    Ghostty,
    Kitty,
    Foot,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Shape {
    #[default]
    Default,
    Columns,
    Rows,
}

impl Terminal {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Ghostty => "ghostty",
            Self::Kitty => "kitty",
            Self::Foot => "foot",
        }
    }

    pub(crate) fn encode(self, shape: Shape) -> Vec<u8> {
        let name = match (shape, self) {
            (Shape::Columns, _) => "ew-resize",
            (Shape::Rows, _) => "ns-resize",
            (Shape::Default, Self::Ghostty) => "text",
            (Shape::Default, Self::Kitty | Self::Foot) => "",
        };
        format!("\x1b]22;{name}\x1b\\").into_bytes()
    }

    pub(crate) fn reset(self) -> Vec<u8> {
        self.encode(Shape::Default)
    }
}

pub(crate) fn detect(version: Option<&str>, var: impl Fn(&str) -> Option<String>) -> Option<Terminal> {
    if in_multiplexer(var) {
        return None;
    }
    let lower = version?.to_ascii_lowercase();
    let at_least = |rest: &str, min| parse_version(rest).is_some_and(|v| v >= min);
    if let Some(rest) = lower.strip_prefix("ghostty ") {
        at_least(rest, MIN_GHOSTTY).then_some(Terminal::Ghostty)
    } else if let Some(rest) = lower.strip_prefix("kitty(") {
        at_least(rest, MIN_KITTY).then_some(Terminal::Kitty)
    } else if let Some(rest) = lower.strip_prefix("foot(") {
        at_least(rest, MIN_FOOT).then_some(Terminal::Foot)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
