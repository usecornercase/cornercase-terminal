#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Glyphs {
    pub(crate) menu: &'static str,
    pub(crate) changes: &'static str,
    pub(crate) files: &'static str,
    pub(crate) todo: &'static str,
    pub(crate) search: &'static str,
    pub(crate) filter: &'static str,
    pub(crate) names: &'static str,
}

pub(crate) const PLAIN: Glyphs =
    Glyphs { menu: "≡", changes: "±", files: "▤", todo: "☐", search: "⌕", filter: "⌕", names: "▤" };

pub(crate) const NERD: Glyphs = Glyphs {
    menu: "\u{eb94}",
    changes: "\u{ea68}",
    files: "\u{eaf0}",
    todo: "\u{eab3}",
    search: "\u{ea6d}",
    filter: "\u{eaf1}",
    names: "\u{ea7b}",
};

pub(crate) fn glyphs(nerd: bool) -> &'static Glyphs {
    if nerd { &NERD } else { &PLAIN }
}

pub(crate) const ISSUES: &str = "\u{eb0c}";

pub(crate) const UPDATE: &str = "\u{eac2}";

pub(crate) const REMOTE: &str = "\u{eb3a}";

pub(crate) const USAGE: &str = "\u{eb03}";

pub(crate) const SETTINGS: &str = "\u{eb51}";

pub(crate) const QUIT: &str = "\u{ea6e}";

pub(crate) const DOT: &str = "\u{f09df}";

const COUNTS: [&str; 9] = [
    "\u{f0ca0}",
    "\u{f0ca2}",
    "\u{f0ca4}",
    "\u{f0ca6}",
    "\u{f0ca8}",
    "\u{f0caa}",
    "\u{f0cac}",
    "\u{f0cae}",
    "\u{f0cb0}",
];

const MANY: &str = "\u{f0cb2}";

pub(crate) fn count(n: usize) -> &'static str {
    n.checked_sub(1).and_then(|i| COUNTS.get(i)).copied().unwrap_or(MANY)
}
