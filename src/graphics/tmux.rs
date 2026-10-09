pub const LIMIT: usize = 1 << 20;

pub fn wrap(sequence: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sequence.len() + 16);
    out.extend_from_slice(b"\x1bPtmux;");
    for &byte in sequence {
        if byte == 0x1b {
            out.push(0x1b);
        }
        out.push(byte);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::kitty_command(b"\x1b_Ga=d,d=I,q=2,i=1\x1b\\", b"\x1bPtmux;\x1b\x1b_Ga=d,d=I,q=2,i=1\x1b\x1b\\\x1b\\")]
    #[case::iterm_file(b"\x1b]1337;File=inline=1:AAAA\x07", b"\x1bPtmux;\x1b\x1b]1337;File=inline=1:AAAA\x07\x1b\\")]
    #[case::no_escape(b"text", b"\x1bPtmux;text\x1b\\")]
    #[case::empty(b"", b"\x1bPtmux;\x1b\\")]
    fn doubles_every_escape_inside_a_passthrough_string(#[case] sequence: &[u8], #[case] expected: &[u8]) {
        assert_eq!(wrap(sequence), expected);
    }

    fn unwrap_like_tmux(wrapped: &[u8]) -> (Vec<u8>, usize) {
        let body = wrapped.strip_prefix(b"\x1bPtmux;").expect("a tmux passthrough string");
        let mut out = Vec::new();
        let mut bytes = body.iter().enumerate();
        while let Some((_, &byte)) = bytes.next() {
            if byte != 0x1b {
                out.push(byte);
                continue;
            }
            match bytes.next() {
                Some((_, 0x1b)) => out.push(0x1b),
                Some((at, b'\\')) => return (out, body.len() - at - 1),
                other => panic!("tmux drops a lone escape before {other:?}"),
            }
        }
        panic!("the string never ends");
    }

    #[test]
    fn tmux_gets_back_the_sequence_and_nothing_follows_its_end() {
        let sequence = b"\x1b_Ga=t,q=2,i=1,m=1;AAAA\x1b\\";

        assert_eq!(unwrap_like_tmux(&wrap(sequence)), (sequence.to_vec(), 0));
    }
}
