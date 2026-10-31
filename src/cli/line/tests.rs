use std::io::Read as _;

use super::*;
use crate::cli::{note, print_json, print_out};

mod a_line {
    use super::*;

    #[test]
    fn takes_what_print_json_prints_as_a_value() {
        let (result, printed) = run(None, || print_json(&serde_json::json!({"a": 1})).map(|()| true));

        assert!(result.expect("ran"));
        assert_eq!(printed.json, Some(serde_json::json!({"a": 1})));
        assert_eq!(printed.text, "");
    }

    #[test]
    fn a_second_json_goes_to_the_text() {
        let (_, printed) = run(None, || {
            print_json(&1)?;
            print_json(&2).map(|()| true)
        });

        assert_eq!((printed.json, printed.text.as_str()), (Some(serde_json::json!(1)), "2\n"));
    }

    #[test]
    fn takes_text_and_notes_apart() {
        let (_, printed) = run(None, || {
            print_out("12\n")?;
            note("warning: x\n");
            Ok(true)
        });

        assert_eq!((printed.text.as_str(), printed.stderr.as_str()), ("12\n", "warning: x\n"));
    }

    #[test]
    fn gives_its_stdin_once_then_nothing() {
        let (_, printed) = run(Some("hello".into()), || {
            let mut first = String::new();
            input().read_to_string(&mut first)?;
            let mut second = String::new();
            input().read_to_string(&mut second)?;
            print_out(&format!("{first}|{second}"))?;
            Ok(true)
        });

        assert_eq!(printed.text, "hello|");
    }

    #[test]
    fn a_command_without_stdin_reads_nothing() {
        let (_, printed) = run(None, || {
            let mut text = String::new();
            input().read_to_string(&mut text)?;
            print_out(&format!("[{text}]"))?;
            Ok(true)
        });

        assert_eq!(printed.text, "[]");
    }
}

#[test]
fn outside_a_line_nothing_is_taken() {
    assert!(!json("1\n"));
    assert!(!text("x"));
    assert!(!stderr("x"));
}
