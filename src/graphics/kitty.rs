pub fn id(hi: u8, lo: u8) -> u32 {
    (u32::from(hi) << 24) | u32::from(lo)
}

pub fn place(id: u32, cols: u16, rows: u16) -> Vec<u8> {
    let _ = (id, cols, rows);
    Vec::new()
}

pub fn delete(id: u32) -> Vec<u8> {
    let _ = id;
    Vec::new()
}

pub fn cell(id: u32, row: u16, col: u16) -> (String, u8) {
    let _ = (row, col);
    (String::new(), u8::try_from(id & 0xff).unwrap_or(0))
}
