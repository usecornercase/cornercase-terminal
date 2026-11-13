use std::io;

use serde_json::json;

use super::*;

struct Left;

impl Write for Left {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

mod writing_the_lines {
    use super::*;

    #[test]
    fn writes_each_line_as_compact_json() {
        let mut out = Vec::new();

        let ended = write_line(&mut out, &json!({"order": "open", "port": 3000})).expect("written");

        assert!(!ended);
        assert_eq!(String::from_utf8(out).expect("text"), "{\"order\":\"open\",\"port\":3000}\n");
    }

    #[test]
    fn stops_once_the_reader_left() {
        assert!(write_line(&mut Left, &json!({"order": "open"})).expect("quiet"));
    }
}
