use super::{CellSize, Picture, Protocol, Tmux};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    pub cols: u16,
    pub rows: u16,
    pub width: u32,
    pub height: u32,
}

pub fn fit(width: u32, height: u32, cell: Option<CellSize>, room: (u16, u16)) -> Option<Fit> {
    let _ = (width, height, cell, room);
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub picture: u64,
    pub protocol: Protocol,
    pub tmux: Tmux,
    pub fit: (u16, u16, u32, u32),
    pub background: Option<(u8, u8, u8)>,
    pub kitty_id: u32,
}

pub fn encode(picture: &Picture, key: &Key) -> Vec<u8> {
    let _ = (picture, key);
    Vec::new()
}
