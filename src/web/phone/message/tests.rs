use rstest::rstest;

use super::*;

const DATA: &str = "bad message: not one of `argv`, `stdin`, `stdin_base64` and `eof` with a value of its type";
const SYNTAX: &str = "bad message: not one JSON object with one key";

mod the_first_message {
    use super::*;

    #[test]
    fn is_an_argv() {
        assert_eq!(first(r#"{"argv":["status","--json"]}"#), Ok(vec!["status".to_string(), "--json".to_string()]));
    }

    #[rstest]
    #[case::invalid("nope", SYNTAX)]
    #[case::an_array("[]", SYNTAX)]
    #[case::empty("{}", SYNTAX)]
    #[case::trailing_text(r#"{"eof":true} x"#, SYNTAX)]
    #[case::unknown(r#"{"x":1}"#, DATA)]
    #[case::two_keys(r#"{"stdin":"a","eof":true}"#, SYNTAX)]
    #[case::wrong_type(r#"{"argv":"x"}"#, DATA)]
    #[case::cut("{\"argv\":[", "bad message: the JSON ends too early")]
    #[case::stdin_first(r#"{"stdin":"a"}"#, "the first message must be `argv`")]
    fn refuses_anything_else(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(first(text), Err(expected.to_string()));
    }
}

mod later_messages {
    use super::*;

    #[rstest]
    #[case::text(r#"{"stdin":"hello"}"#, Input::Text("hello".into()))]
    #[case::bytes(r#"{"stdin_base64":"aGk="}"#, Input::Bytes(b"hi".to_vec()))]
    #[case::eof(r#"{"eof":true}"#, Input::Eof)]
    fn are_accepted(#[case] text: &str, #[case] expected: Input) {
        assert_eq!(next(text), Ok(expected));
    }

    #[rstest]
    #[case::second_argv(r#"{"argv":[]}"#, "`argv` was already sent")]
    #[case::eof_false(r#"{"eof":false}"#, "bad message: `eof` must be true")]
    #[case::unpadded(r#"{"stdin_base64":"aGk"}"#, "bad message: `stdin_base64` is not standard base64")]
    #[case::url_alphabet(r#"{"stdin_base64":"-_-_"}"#, "bad message: `stdin_base64` is not standard base64")]
    fn are_refused(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(next(text), Err(expected.to_string()));
    }

    #[test]
    fn text_is_written_with_a_newline_and_eof_writes_nothing() {
        assert_eq!(Input::Text("a".into()).into_bytes(), Some(b"a\n".to_vec()));
        assert_eq!(Input::Bytes(vec![1, 2]).into_bytes(), Some(vec![1, 2]));
        assert_eq!(Input::Eof.into_bytes(), None);
    }
}

mod the_output {
    use super::*;

    #[test]
    fn serialises_as_one_key() {
        assert_eq!(Output::Stdout("a".into()).json(), r#"{"stdout":"a"}"#);
        assert_eq!(Output::Stderr("a".into()).json(), r#"{"stderr":"a"}"#);
        assert_eq!(Output::Exit(-1).json(), r#"{"exit":-1}"#);
    }

    #[test]
    fn a_refusal_has_no_control_characters() {
        assert_eq!(refusal("a\nb\tc"), "a b c");
    }
}
