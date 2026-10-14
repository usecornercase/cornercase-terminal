use super::*;

#[test]
fn an_unterminated_osc_cannot_grow_the_buffer_past_the_limit() {
    let mut stream = Stream::default();
    for byte in b"\x1b]".iter().copied().chain(std::iter::repeat_n(b'a', 100_000)) {
        assert!(stream.advance(byte).is_none());
    }
    assert_eq!(stream.bytes.len(), MAX_SEQUENCE);
    assert!(stream.advance(0x07).is_none());
}
