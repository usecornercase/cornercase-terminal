use crossterm::event::{KeyEvent, KeyEventState, KeyModifiers, MouseEvent};
use rstest::rstest;

use super::*;

const SCREEN: Rect = Rect { x: 0, y: 0, width: 80, height: 24 };

fn machine(destination: &str, command: Option<&str>) -> Machine {
    Machine { destination: destination.into(), command: command.map(Into::into) }
}

fn on(destination: &str) -> Target {
    Target::Remote(machine(destination, None).remote())
}

fn known() -> Vec<Machine> {
    vec![machine("devbox", Some("~/tools/cornercase")), machine("gpu-box", None)]
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
    fn this_machine_is_reached_without_asking() {
        let (to, asks) = chosen(None, &on("devbox"), known()).expect("a switch");

        assert!(matches!(to, Target::Local) && !asks);
    }

    #[test]
    fn a_known_machine_is_reached_without_asking_and_with_its_command() {
        let (to, asks) = chosen(Some("devbox".into()), &Target::Local, known()).expect("a switch");

        let Target::Remote(remote) = to else { panic!("a remote target") };
        assert_eq!((remote.command.as_deref(), asks), (Some("~/tools/cornercase"), false));
    }

    #[test]
    fn a_machine_it_never_reached_asks_first() {
        let (to, asks) = chosen(Some("new-box".into()), &Target::Local, known()).expect("a switch");

        assert_eq!((to.machine(), asks), (Some("new-box"), true));
    }

    #[rstest]
    #[case::this_machine_from_this_machine(None, Target::Local)]
    #[case::the_machine_it_is_on(Some("devbox"), on("devbox"))]
    #[case::an_option(Some("-oProxyCommand=sh"), Target::Local)]
    #[case::two_words(Some("devbox ls"), Target::Local)]
    fn nothing_happens(#[case] to: Option<&str>, #[case] here: Target) {
        assert!(chosen(to.map(Into::into), &here, known()).is_none());
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
    fn the_local_machine_is_this_machine() {
        assert_eq!([place(&Target::Local), place(&on("devbox"))], [THIS_MACHINE, "devbox"]);
    }
}
