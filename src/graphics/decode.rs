use super::Picture;

pub fn sniff(head: &[u8]) -> Option<&'static str> {
    let _ = head;
    None
}

pub fn decode(bytes: &[u8], id: u64) -> Result<Picture, String> {
    let _ = (bytes, id);
    Err("not built yet".to_string())
}
