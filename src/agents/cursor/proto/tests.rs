use super::*;

#[test]
fn reads_varints_and_nested_bytes() {
    let data = [0x08, 0x96, 0x01, 0x12, 0x03, b'a', b'b', b'c'];
    let found = fields(&data).expect("well formed");
    assert_eq!(number(&found, 1), Some(150));
    assert_eq!(text(&found, 2).as_deref(), Some("abc"));
}

#[test]
fn skips_fixed_width_fields_it_does_not_know() {
    let data = [0x0d, 1, 2, 3, 4, 0x11, 1, 2, 3, 4, 5, 6, 7, 8, 0x18, 0x07];
    let found = fields(&data).expect("well formed");
    assert_eq!((found.len(), number(&found, 3)), (1, Some(7)));
}

#[test]
fn refuses_truncated_or_unknown_data() {
    assert!(fields(&[0x12, 0x05, b'a']).is_none());
    assert!(fields(&[0x08, 0x80]).is_none());
    assert!(fields(&[0x0b]).is_none());
    assert!(fields(&[0x00, 0x01]).is_none());
}

#[test]
fn names_a_blob_by_its_raw_or_hex_bytes() {
    assert_eq!(blob_id(&[0xab; 32]).as_deref(), Some(&*"ab".repeat(32)));
    assert_eq!(blob_id("AB".repeat(32).as_bytes()).as_deref(), Some(&*"ab".repeat(32)));
    assert_eq!(blob_id(b"short"), None);
}

#[test]
fn decodes_hex() {
    assert_eq!(from_hex("7b7d"), Some(b"{}".to_vec()));
    assert_eq!(from_hex("7b7"), None);
    assert_eq!(from_hex("zz"), None);
}
