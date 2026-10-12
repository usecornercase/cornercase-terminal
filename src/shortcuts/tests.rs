use rstest::rstest;

use super::*;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

mod prefix {
    use super::*;

    #[rstest]
    #[case::ctrl_bracket("ctrl+]")]
    #[case::ctrl_space("ctrl+space")]
    #[case::alt_letter("alt+k")]
    #[case::ctrl_shift_letter("ctrl+shift+k")]
    #[case::function_key("f12")]
    #[case::ctrl_function_key("ctrl+f5")]
    fn reads_back_what_it_writes(#[case] text: &str) {
        assert_eq!(Prefix::parse(text).map(|p| p.to_string()).as_deref(), Some(text));
    }

    #[rstest]
    #[case::empty("")]
    #[case::plain_letter("k")]
    #[case::unknown_modifier("hyper+k")]
    #[case::two_keys("ctrl+ab")]
    #[case::ctrl_c("ctrl+c")]
    #[case::shift_only("shift+k")]
    fn refuses_what_cannot_be_one(#[case] text: &str) {
        assert_eq!(Prefix::parse(text), None);
    }

    #[test]
    fn is_read_ignoring_case_and_spaces() {
        assert_eq!(Prefix::parse(" Ctrl+Space ").map(|p| p.to_string()).as_deref(), Some("ctrl+space"));
    }

    #[rstest]
    #[case::legacy_byte(KeyCode::Char('5'), KeyModifiers::CONTROL)]
    #[case::kitty(KeyCode::Char(']'), KeyModifiers::CONTROL)]
    fn ctrl_bracket_matches_however_the_terminal_sends_it(#[case] code: KeyCode, #[case] mods: KeyModifiers) {
        let prefix = Prefix::parse("ctrl+]").expect("a prefix");
        assert!(prefix.matches(key(code, mods)));
    }

    #[test]
    fn an_upper_case_letter_is_that_letter_with_shift() {
        let prefix = Prefix::parse("ctrl+shift+k").expect("a prefix");
        assert!(prefix.matches(key(KeyCode::Char('K'), KeyModifiers::CONTROL)));
    }

    #[test]
    fn shift_on_a_symbol_is_part_of_the_symbol() {
        let prefix = Prefix::from_key(key(KeyCode::Char('}'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
        assert_eq!(prefix.map(|p| p.to_string()), Ok("ctrl+}".into()));
    }

    #[test]
    fn another_modifier_does_not_match() {
        let prefix = Prefix::parse("ctrl+]").expect("a prefix");
        assert!(!prefix.matches(key(KeyCode::Char(']'), KeyModifiers::CONTROL | KeyModifiers::ALT)));
    }

    #[rstest]
    #[case::enter(KeyCode::Enter, KeyModifiers::NONE)]
    #[case::letter(KeyCode::Char('k'), KeyModifiers::NONE)]
    #[case::arrow(KeyCode::Left, KeyModifiers::CONTROL)]
    fn a_pressed_key_that_cannot_be_one_says_why(#[case] code: KeyCode, #[case] mods: KeyModifiers) {
        assert_eq!(Prefix::from_key(key(code, mods)), Err("use ctrl or alt with a key, or a function key".into()));
    }

    #[test]
    fn ctrl_c_says_what_it_does() {
        let refused = Prefix::from_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(refused, Err("ctrl+c cannot be the prefix: it stops programs".into()));
    }

    #[test]
    fn known_clashes_are_named() {
        let clash = Prefix::parse("ctrl+b").and_then(Prefix::clash);
        assert_eq!(clash, Some("tmux's prefix, so it never reaches a tmux inside a pane"));
    }
}

mod keys {
    use super::*;

    fn press(group: Option<Group>, c: char) -> Option<Step> {
        lookup(group, key(KeyCode::Char(c), KeyModifiers::NONE))
    }

    #[test]
    fn a_letter_runs_its_action() {
        assert_eq!(press(None, 'n'), Some(Step::Run(Action::NextTab)));
    }

    #[test]
    fn a_group_letter_opens_the_group() {
        assert_eq!(press(None, 'f'), Some(Step::Open(Group::Find)));
    }

    #[test]
    fn a_letter_means_what_its_group_says() {
        assert_eq!(press(Some(Group::Find), 'f'), Some(Step::Run(Action::FindNames)));
    }

    #[test]
    fn digits_go_to_a_tab_from_one() {
        assert_eq!(press(None, '1'), Some(Step::Run(Action::Tab(0))));
    }

    #[test]
    fn arrows_move_between_panes() {
        assert_eq!(lookup(None, key(KeyCode::Up, KeyModifiers::NONE)), Some(Step::Run(Action::Pane(Place::Above))));
    }

    #[test]
    fn an_unknown_key_does_nothing() {
        assert_eq!(press(Some(Group::Git), 'z'), None);
    }

    #[test]
    fn a_key_with_ctrl_does_nothing() {
        assert_eq!(lookup(None, key(KeyCode::Char('n'), KeyModifiers::CONTROL)), None);
    }

    #[test]
    fn no_key_is_listed_twice_in_a_menu() {
        for group in [None, Some(Group::Find), Some(Group::Git), Some(Group::Workspace)] {
            let mut keys: Vec<&str> = items(group).iter().map(|i| i.key).collect();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), items(group).len(), "{group:?}");
        }
    }
}
