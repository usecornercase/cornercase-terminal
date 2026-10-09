use serde::{Deserialize, Serialize};

pub mod decode;
pub mod detect;
pub mod encode;
pub mod iterm;
pub mod kitty;
pub mod sixel;
pub mod tmux;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Support {
    pub protocol: Option<Protocol>,
    pub missing: Option<Missing>,
    pub cell: Option<CellSize>,
    pub tmux: Tmux,
    pub id_hi: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    Kitty,
    Iterm,
    Sixel { registers: u16, max: Option<(u32, u32)> },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tmux {
    #[default]
    None,
    Wrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellSize {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Missing {
    Unknown { name: Option<String> },
    Cannot { name: String },
    Off,
}

impl Missing {
    pub fn lines(&self) -> Vec<String> {
        match self {
            Self::Unknown { .. } | Self::Cannot { .. } | Self::Off => {
                vec!["this terminal cannot show images".to_string()]
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub id: u64,
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub rgba: image::RgbaImage,
    pub opaque: bool,
}
