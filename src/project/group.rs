use serde::{Deserialize, Serialize};

pub(crate) const GROUP_ICONS: [char; 12] = ['●', '◉', '◐', '◆', '■', '▲', '▼', '★', '✦', '♥', '♣', '♠'];

pub(crate) const GROUP_COLOURS: [u8; 16] = [1, 9, 208, 214, 3, 11, 2, 10, 6, 14, 4, 12, 99, 5, 13, 205];

pub const GROUP_STYLES: [(char, u8); 8] =
    [('●', 12), ('◆', 2), ('★', 99), ('♥', 9), ('▲', 208), ('■', 214), ('✦', 205), ('◉', 6)];

pub(crate) struct Group {
    pub(crate) id: u64,
    pub(crate) entry: GroupEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupEntry {
    pub(crate) name: String,
    pub(crate) icon: char,
    pub(crate) colour: u8,
    #[serde(default)]
    pub(crate) collapsed: bool,
}

impl GroupEntry {
    pub(crate) fn label(&self) -> String {
        format!("{} {}", self.icon, self.name)
    }
}

pub(crate) fn preset(groups: &[Group]) -> (char, u8) {
    let taken = |&(icon, colour): &(char, u8)| groups.iter().any(|g| (g.entry.icon, g.entry.colour) == (icon, colour));
    GROUP_STYLES.iter().copied().find(|style| !taken(style)).unwrap_or(GROUP_STYLES[groups.len() % GROUP_STYLES.len()])
}

pub(crate) fn known_icon(icon: char) -> char {
    if GROUP_ICONS.contains(&icon) { icon } else { GROUP_ICONS[0] }
}
