use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub id: u64,
    pub label: String,
    pub files: Range<usize>,
    pub workspace: bool,
    pub folded: bool,
}
