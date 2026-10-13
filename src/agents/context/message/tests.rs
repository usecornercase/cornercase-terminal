use std::io::Write;

use super::*;
use crate::test_util::TempDir;

pub(in crate::agents) struct Written {
    _dir: TempDir,
    pub(in crate::agents) path: PathBuf,
}

impl Written {
    pub(in crate::agents) fn new(text: &str) -> Self {
        let dir = TempDir::new();
        let path = dir.path().join("record.jsonl");
        std::fs::write(&path, text).expect("write the record");
        Self { _dir: dir, path }
    }

    fn lines(&self) -> Vec<String> {
        let lines = Backwards::open(&self.path).expect("open the record");
        lines.map(|line| String::from_utf8(line.expect("a line")).expect("text")).collect()
    }

    pub(in crate::agents) fn append(&self, text: &str) {
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path).expect("open the record");
        file.write_all(text.as_bytes()).expect("append to the record");
    }
}

mod backwards {
    use super::*;

    #[test]
    fn gives_the_newest_line_first() {
        let file = Written::new("one\ntwo\n\nthree\n");

        assert_eq!(file.lines(), ["three", "two", "one"]);
    }

    #[test]
    fn leaves_out_a_line_still_being_written() {
        let file = Written::new("one\ntwo\n");
        file.append("{\"half");

        assert_eq!(file.lines(), ["two", "one"]);
    }

    #[test]
    fn joins_lines_cut_by_its_chunks() {
        let long: Vec<String> = (0..5).map(|n| format!("{n}{}", "x".repeat(200_000))).collect();
        let file = Written::new(&joined_lines(&long));

        let read = file.lines();

        assert_eq!(read, long.iter().rev().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn a_file_of_one_unfinished_line_has_none() {
        let file = Written::new(&"x".repeat(600_000));

        assert_eq!(file.lines(), Vec::<String>::new());
    }

    #[test]
    fn stops_after_its_budget() {
        let line = "y".repeat(1024 * 1024 - 1);
        let file = Written::new(&format!("first\n{}", format!("{line}\n").repeat(64)));

        let read = file.lines();

        assert_eq!((read.len(), read.contains(&"first".to_string())), (63, false));
    }
}

pub(in crate::agents) fn joined_lines(lines: &[String]) -> String {
    lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect()
}
