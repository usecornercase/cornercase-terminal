use rstest::rstest;

use super::*;

#[rstest]
#[case::empty("", "")]
#[case::one_byte("f", "Zg==")]
#[case::two_bytes("fo", "Zm8=")]
#[case::three_bytes("foo", "Zm9v")]
#[case::url("https://x.dev/1", "aHR0cHM6Ly94LmRldi8x")]
#[case::non_ascii("ñ", "w7E=")]
fn encodes_like_rfc_4648(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(base64(input.as_bytes()), expected);
}

#[test]
fn osc52_asks_the_terminal_to_set_the_clipboard() {
    assert_eq!(osc52("foo"), b"\x1b]52;c;Zm9v\x07");
}
