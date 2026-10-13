use serde::{Deserialize, Serialize};

pub const GROUP_ICONS: [char; 12] = ['●', '◉', '◐', '◆', '■', '▲', '▼', '★', '✦', '♥', '♣', '♠'];

pub const GROUP_COLOURS: [u8; 16] = [1, 9, 208, 214, 3, 11, 2, 10, 6, 14, 4, 12, 99, 5, 13, 205];

pub const GROUP_STYLES: [(char, u8); 8] =
    [('●', 12), ('◆', 2), ('★', 99), ('♥', 9), ('▲', 208), ('■', 214), ('✦', 205), ('◉', 6)];

pub struct Group {
    pub id: u64,
    pub entry: GroupEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupEntry {
    pub name: String,
    pub icon: char,
    pub colour: u8,
    #[serde(default)]
    pub collapsed: bool,
}

impl GroupEntry {
    pub fn label(&self) -> String {
        format!("{} {}", self.icon, self.name)
    }
}
