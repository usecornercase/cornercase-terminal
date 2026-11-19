use crossterm::event::{KeyEvent, KeyEventState, KeyModifiers, MouseEvent};
use rstest::rstest;

use super::*;

const SCREEN: Rect = Rect { x: 0, y: 0, width: 80, height: 24 };

fn machine(name: &str, destination: &str, command: Option<&str>) -> Machine {
    Machine { command: command.map(Into::into), ..machines::machine(name, destination) }
}

fn on(destination: &str) -> Target {
    Target::Remote(machine("x", destination, None).remote())
}

fn known() -> Vec<Machine> {
    vec![machine("devbox", "me@devbox", Some("~/tools/cornercase")), machine("gpu-box", "gpu.lan", None)]
}

fn key(code: KeyCode, kind: KeyEventKind) -> Event {
    Event::Key(KeyEvent { code, modifiers: KeyModifiers::NONE, kind, state: KeyEventState::NONE })
}

fn mouse(kind: MouseEventKind, button: Button, step: &Step) -> Event {
    let shown = if *step == Step::Asking { [Button::Connect, Button::Back].as_slice() } else { &[Button::Back] };
    let (_, r) = notice::buttons(SCREEN, shown).into_iter().find(|(b, _)| *b == button).expect("the button");
    Event::Mouse(MouseEvent { kind, column: r.x, row: r.y, modifiers: KeyModifiers::NONE })
}

mod choosing {
    use super::*;

    #[test]
    fn this_computer_is_reached_without_asking() {
        let (to, asks) = chosen(None, &on("me@devbox"), &known()).expect("a switch");

        assert!(matches!(to, Target::Local) && !asks);
    }

    #[test]
    fn a_machine_is_found_by_its_name_whatever_its_case_and_reached_with_its_command() {
        let (to, asks) = chosen(Some("DevBox".into()), &Target::Local, &known()).expect("a switch");

        let Target::Remote(remote) = to else { panic!("a remote target") };
        assert_eq!((remote.destination.as_str(), remote.command.as_deref()), ("me@devbox", Some("~/tools/cornercase")));
        assert!(!asks);
    }

    #[test]
    fn a_machine_a_program_added_asks_first() {
        let list = vec![Machine { unreached: true, ..machine("newbox", "newbox", Some("~/bin/cc")) }];

        let (to, asks) = chosen(Some("newbox".into()), &Target::Local, &list).expect("a switch");

        let Target::Remote(remote) = to else { panic!("a remote target") };
        assert_eq!((remote.command.as_deref(), asks), (Some("~/bin/cc"), true));
    }

    #[test]
    fn a_machine_with_a_problem_is_never_chosen() {
        let list = vec![machine("my box", "newbox", None)];

        assert!(chosen(Some("my box".into()), &Target::Local, &list).is_none());
    }

    #[rstest]
    #[case::this_computer_from_this_computer(None, Target::Local)]
    #[case::the_machine_it_is_on(Some("devbox"), on("me@devbox"))]
    #[case::a_name_that_is_not_listed(Some("nobox"), Target::Local)]
    #[case::a_destination_instead_of_a_name(Some("me@devbox"), Target::Local)]
    fn nothing_happens(#[case] to: Option<&str>, #[case] here: Target) {
        assert!(chosen(to.map(Into::into), &here, &known()).is_none());
    }
}

mod listing {
    use super::*;

    #[test]
    fn names_go_in_the_order_of_the_list_and_problems_are_left_out() {
        let mut list = known();
        list.push(machine("bad name", "bad.lan", None));

        let sent = machines_of(&list, &Target::Local);

        assert_eq!((sent.here, sent.known), (None, vec!["devbox".to_string(), "gpu-box".to_string()]));
    }

    #[rstest]
    #[case::a_listed_one("gpu.lan", "gpu-box")]
    #[case::an_unlisted_one("far.lan", "far.lan")]
    fn the_machine_it_is_on_is_named_by_its_entry_else_by_its_destination(#[case] at: &str, #[case] named: &str) {
        assert_eq!(machines_of(&known(), &on(at)).here.as_deref(), Some(named));
    }
}

mod pressing {
    use super::*;

    #[test]
    fn esc_goes_back_whatever_the_step() {
        for step in [Step::Asking, Step::Connecting, Step::Failed("refused".into())] {
            assert_eq!(pressed(&step, &key(KeyCode::Esc, KeyEventKind::Press), SCREEN), Some(Pressed::Back));
        }
    }

    #[test]
    fn enter_connects_only_while_it_asks() {
        let enter = key(KeyCode::Enter, KeyEventKind::Press);

        assert_eq!(
            [pressed(&Step::Asking, &enter, SCREEN), pressed(&Step::Connecting, &enter, SCREEN)],
            [Some(Pressed::Connect), None]
        );
    }

    #[test]
    fn a_released_key_does_nothing() {
        assert_eq!(pressed(&Step::Asking, &key(KeyCode::Esc, KeyEventKind::Release), SCREEN), None);
    }

    #[test]
    fn a_press_on_connect_connects() {
        let press = mouse(MouseEventKind::Down(MouseButton::Left), Button::Connect, &Step::Asking);

        assert_eq!(pressed(&Step::Asking, &press, SCREEN), Some(Pressed::Connect));
    }

    #[test]
    fn a_press_on_back_goes_back() {
        let step = Step::Failed("refused".into());
        let press = mouse(MouseEventKind::Down(MouseButton::Left), Button::Back, &step);

        assert_eq!(pressed(&step, &press, SCREEN), Some(Pressed::Back));
    }

    #[test]
    fn moving_over_a_button_lights_it() {
        let over = mouse(MouseEventKind::Moved, Button::Back, &Step::Connecting);

        assert_eq!(pressed(&Step::Connecting, &over, SCREEN), Some(Pressed::Hover(Some(Button::Back))));
    }

    #[test]
    fn connect_is_not_there_once_it_connects() {
        let press = mouse(MouseEventKind::Down(MouseButton::Left), Button::Connect, &Step::Asking);

        assert_eq!(pressed(&Step::Connecting, &press, SCREEN), Some(Pressed::Hover(None)));
    }
}

mod failing {
    use super::*;

    #[test]
    fn ssh_s_own_words_are_shown() {
        let error = Error::Ssh { host: "devbox".into(), reason: "Permission denied (publickey)".into() };

        assert_eq!(failure(&error), "Permission denied (publickey)");
    }

    #[test]
    fn a_version_apart_says_which_side_to_update() {
        let error = Error::RemoteOlder { host: "devbox".into(), there: "0.0.1".into() };

        assert_eq!(failure(&error), error.to_string());
    }
}

mod naming {
    use super::*;

    #[test]
    fn the_local_computer_is_this_computer() {
        assert_eq!([place(&Target::Local), place(&on("devbox"))], [machines::HERE, "devbox"]);
    }
}

mod answering {
    use super::*;
    use crate::client::Ending;

    #[test]
    fn any_first_message_but_a_rejection_accepts_the_window() {
        let first = answer(Received::Message(ServerMessage::Frame(b"\x1b[2J".to_vec())));

        assert!(matches!(first, Ok(ServerMessage::Frame(_))));
    }

    #[test]
    fn a_rejection_keeps_its_reason() {
        let refused = answer(Received::Message(ServerMessage::Rejected("another build".into())));

        assert!(matches!(refused, Err(Error::Rejected(reason)) if reason == "another build"));
    }

    #[test]
    fn a_link_that_ends_first_says_so() {
        assert!(matches!(answer(Received::Ended), Err(Error::Control(reason)) if reason == CLOSED));
    }

    #[test]
    fn what_cannot_be_read_is_another_build() {
        assert!(matches!(answer(Received::Invalid), Err(Error::Rejected(reason)) if reason == INCOMPATIBLE));
    }

    #[test]
    fn the_first_end_of_the_old_link_wins() {
        let first = kept(None, Next::End(Ending::Restart));

        assert!(matches!(kept(first, Next::End(Ending::Detached)), Some(Next::End(Ending::Restart))));
    }
}
