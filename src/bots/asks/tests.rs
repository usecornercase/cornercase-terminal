use crossterm::event::KeyModifiers;
use rstest::rstest;

use super::*;

const WIDTH: usize = 20;
const TYPED: &str = "hunter2-typed";

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

fn ask(list: &[&str]) -> Ask {
    Ask { id: 3, names: names(list), about: String::from("to log in"), replace: false, run: None, given: Vec::new() }
}

fn press(form: &mut Form, code: KeyCode) -> Step {
    form.key(KeyEvent::new(code, KeyModifiers::NONE), WIDTH)
}

fn typed(form: &mut Form, text: &str) {
    for c in text.chars() {
        press(form, KeyCode::Char(c));
    }
}

mod the_names {
    use super::*;

    #[rstest]
    #[case::none(&[])]
    #[case::five(&["a", "b", "c", "d", "e"])]
    #[case::a_bad_name(&["Not A Name"])]
    #[case::a_name_twice(&["a", "a"])]
    fn are_refused(#[case] list: &[&str]) {
        assert!(names_refusal(&names(list)).is_some());
    }

    #[test]
    fn never_echo_a_bad_one() {
        let why = names_refusal(&names(&["hunter2 value"])).expect("refused");
        assert!(!why.contains("hunter2"), "{why}");
    }

    #[test]
    fn take_one_to_four() {
        assert_eq!(names_refusal(&names(&["a"])), None);
        assert_eq!(names_refusal(&names(&["a", "b", "c", "d"])), None);
    }
}

mod the_about {
    use super::*;

    #[rstest]
    #[case::empty(String::new())]
    #[case::too_long("a".repeat(201))]
    #[case::a_line_break(String::from("one\ntwo"))]
    #[case::an_escape(String::from("one\u{1b}[31m"))]
    #[case::a_direction_mark(String::from("one\u{202E}owt"))]
    #[case::an_isolate(String::from("one\u{2067}two"))]
    fn is_refused(#[case] about: String) {
        assert!(about_refusal(&about).is_some());
    }

    #[test]
    fn says_its_size_without_echoing_it() {
        let why = about_refusal(&"x".repeat(201)).expect("refused");
        assert!(why.contains("201"), "{why}");
        assert!(!why.contains("xxx"), "{why}");
    }

    #[test]
    fn takes_two_hundred_characters() {
        assert_eq!(about_refusal(&"é".repeat(200)), None);
    }
}

mod an_ask {
    use super::*;

    #[test]
    fn is_the_same_in_any_order() {
        assert!(ask(&["a", "b"]).same(&names(&["b", "a"])));
        assert!(!ask(&["a", "b"]).same(&names(&["a"])));
    }

    #[test]
    fn misses_what_was_not_given() {
        let mut asked = ask(&["a", "b", "c"]);
        asked.given = names(&["b"]);
        assert_eq!(asked.missing(), vec!["a", "c"]);
        assert!(!asked.done());
        asked.given = names(&["a", "b", "c"]);
        assert!(asked.done());
    }

    #[test]
    fn words_its_texts() {
        assert_eq!(asks_text("a, b"), "asks for a secret: a, b");
        assert_eq!(told_saved("a, b"), "The user saved the secrets you asked for: a, b.");
        assert_eq!(told_declined("a"), "The user did not give: a.");
        assert_eq!(announced("PRs", "a"), "PRs asks for a secret: a; open cornercase to give it");
    }
}

mod the_form {
    use super::*;

    #[test]
    fn has_a_field_for_each_missing_name() {
        let mut asked = ask(&["a", "b"]);
        asked.given = names(&["a"]);
        let form = Form::new(1, &asked);
        assert_eq!(form.fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(), vec!["b"]);
        assert!(form.fields[0].value.is_empty());
    }

    #[test]
    fn tab_wraps_around() {
        let mut form = Form::new(1, &ask(&["a", "b"]));
        press(&mut form, KeyCode::Tab);
        assert_eq!(form.focus, 1);
        press(&mut form, KeyCode::Tab);
        assert_eq!(form.focus, 0);
        press(&mut form, KeyCode::BackTab);
        assert_eq!(form.focus, 1);
    }

    #[test]
    fn enter_moves_to_an_empty_field_before_it_saves() {
        let mut form = Form::new(1, &ask(&["a", "b"]));
        typed(&mut form, "one");
        assert_eq!(press(&mut form, KeyCode::Enter), Step::None);
        assert_eq!(form.focus, 1);
        typed(&mut form, "two");
        assert_eq!(press(&mut form, KeyCode::Enter), Step::Save);
        assert_eq!(form.values()["a"].0, "one");
        assert_eq!(form.values()["b"].0, "two");
    }

    #[test]
    fn esc_is_not_now() {
        let mut form = Form::new(1, &ask(&["a"]));
        assert_eq!(press(&mut form, KeyCode::Esc), Step::Later);
    }

    #[test]
    fn buttons_say_what_they_do() {
        let mut form = Form::new(1, &ask(&["a"]));
        assert_eq!(form.hit(Hit::Button(SAVE), WIDTH), Step::Save);
        assert_eq!(form.hit(Hit::Button(LATER), WIDTH), Step::Later);
        assert_eq!(form.hit(Hit::Button(NEVER), WIDTH), Step::Never);
    }

    #[test]
    fn does_not_debug_a_typed_value() {
        let mut form = Form::new(1, &ask(&["a"]));
        typed(&mut form, TYPED);
        let shown = format!("{form:?} {:?}", form.fields[0]);
        assert!(!shown.contains(TYPED), "{shown}");
        assert!(shown.contains('a'), "{shown}");
    }
}
