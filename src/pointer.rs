use serde::{Deserialize, Serialize};

use crate::graphics::detect::{in_multiplexer, version as parse_version};

const MIN_KITTY: (u32, u32, u32) = (0, 31, 0);
const MIN_FOOT: (u32, u32, u32) = (1, 12, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terminal {
    Ghostty,
    Kitty,
    Foot,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Default,
    Columns,
    Rows,
}

impl Terminal {
    pub fn id(self) -> &'static str {
        match self {
            Self::Ghostty => "ghostty",
            Self::Kitty => "kitty",
            Self::Foot => "foot",
        }
    }

    pub fn encode(self, shape: Shape) -> Vec<u8> {
        let name = match (shape, self) {
            (Shape::Columns, _) => "ew-resize",
            (Shape::Rows, _) => "ns-resize",
            (Shape::Default, Self::Ghostty) => "text",
            (Shape::Default, Self::Kitty | Self::Foot) => "",
        };
        format!("\x1b]22;{name}\x1b\\").into_bytes()
    }

    pub fn reset(self) -> Vec<u8> {
        self.encode(Shape::Default)
    }
}

pub fn detect(version: Option<&str>, var: impl Fn(&str) -> Option<String>) -> Option<Terminal> {
    if in_multiplexer(var) {
        return None;
    }
    let lower = version?.to_ascii_lowercase();
    let at_least = |rest: &str, min| parse_version(rest).is_some_and(|v| v >= min);
    if lower.starts_with("ghostty ") {
        Some(Terminal::Ghostty)
    } else if let Some(rest) = lower.strip_prefix("kitty(") {
        at_least(rest, MIN_KITTY).then_some(Terminal::Kitty)
    } else if let Some(rest) = lower.strip_prefix("foot(") {
        at_least(rest, MIN_FOOT).then_some(Terminal::Foot)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    mod detecting {
        use super::*;

        #[rstest]
        #[case::ghostty("ghostty 1.3.1", Some(Terminal::Ghostty))]
        #[case::kitty("kitty(0.45.0)", Some(Terminal::Kitty))]
        #[case::kitty_first_with_it("kitty(0.31.0)", Some(Terminal::Kitty))]
        #[case::kitty_before_it("kitty(0.30.1)", None)]
        #[case::foot("foot(1.25.0)", Some(Terminal::Foot))]
        #[case::foot_before_it("foot(1.11.0)", None)]
        #[case::wezterm("WezTerm 20240203-110809-5046fc22", None)]
        #[case::iterm2("iTerm2 3.6.6", None)]
        #[case::konsole("Konsole 25.12.0", None)]
        #[case::xterm("XTerm(407)", None)]
        #[case::tmux("tmux 3.6", None)]
        fn only_terminals_known_to_honour_osc_22(#[case] version: &str, #[case] expected: Option<Terminal>) {
            assert_eq!(detect(Some(version), no_env), expected);
        }

        #[test]
        fn nothing_without_an_xtversion_reply() {
            let env = |name: &str| (name == "TERM_PROGRAM").then(|| "ghostty".to_string());

            assert_eq!(detect(None, env), None);
        }

        #[rstest]
        #[case::tmux("TMUX", "/tmp/tmux-501/default,1,0")]
        #[case::screen("STY", "1234.pts-0.host")]
        #[case::zellij("ZELLIJ", "0")]
        fn nothing_inside_a_multiplexer(#[case] var: &str, #[case] value: &str) {
            let env = |name: &str| (name == var).then(|| value.to_string());

            assert_eq!(detect(Some("ghostty 1.3.1"), env), None);
        }
    }

    mod encode {
        use super::*;

        #[rstest]
        #[case::columns(Terminal::Kitty, Shape::Columns, "\x1b]22;ew-resize\x1b\\")]
        #[case::rows(Terminal::Foot, Shape::Rows, "\x1b]22;ns-resize\x1b\\")]
        #[case::kitty_default(Terminal::Kitty, Shape::Default, "\x1b]22;\x1b\\")]
        #[case::foot_default(Terminal::Foot, Shape::Default, "\x1b]22;\x1b\\")]
        #[case::ghostty_default(Terminal::Ghostty, Shape::Default, "\x1b]22;text\x1b\\")]
        fn asks_for_a_css_pointer_name(#[case] terminal: Terminal, #[case] shape: Shape, #[case] expected: &str) {
            assert_eq!(String::from_utf8(terminal.encode(shape)).expect("utf-8"), expected);
        }
    }
}
