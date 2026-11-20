use crossterm::event::{KeyCode, KeyModifiers};
use rstest::rstest;

use super::*;
use crate::machines::machine;

const WIDTH: usize = 50;

fn filled(name: &str, destination: &str, folder: &str) -> Form {
    Form { name: Editor::new(name), destination: Editor::new(destination), folder: Editor::new(folder), ..Form::new() }
}

fn press(form: &mut Form, code: KeyCode) -> Step {
    form.key(KeyEvent::new(code, KeyModifiers::NONE), WIDTH, 8)
}

fn listed() -> Vec<Machine> {
    vec![machine("pi", "pi.lan")]
}

mod a_new_machine {
    use super::*;

    #[test]
    fn starts_with_the_server_icon_and_the_code_folder() {
        let form = Form::new();

        assert_eq!(form.title(), "Settings › Machines › new machine");
        assert_eq!(
            (form.icon.label(), form.folder.text().as_str(), form.buttons()),
            (Some("☁ server"), "~/code", vec![BACK, ADD])
        );
    }

    #[test]
    fn is_added_with_what_was_typed_trimmed() {
        let mut form = filled(" vps ", " me@vps.lan ", " ~/src ");

        let added = Machine {
            name: "vps".into(),
            icon: "☁".into(),
            destination: "me@vps.lan".into(),
            folder: "~/src".into(),
            ..Machine::default()
        };
        assert_eq!(form.checked(&listed()), Some((None, added)));
    }
}

mod an_edited_machine {
    use super::*;

    #[test]
    fn is_filled_in_and_saved_in_its_place() {
        let vps = Machine { icon: "◫".into(), folder: "/srv".into(), ..machine("vps", "vps.lan") };
        let mut form = Form::editing(&vps);

        assert_eq!(
            (form.title().as_str(), form.icon.label(), form.buttons()),
            ("Settings › Machines › vps", Some("◫ mini"), vec![BACK, SAVE])
        );
        assert_eq!([form.name.text(), form.destination.text(), form.folder.text()], ["vps", "vps.lan", "/srv"]);
        form.folder = Editor::new("/opt");
        let saved = Machine { folder: "/opt".into(), ..vps.clone() };
        assert_eq!(form.checked(&[machine("pi", "pi.lan"), vps]), Some((Some(1), saved)));
    }

    #[test]
    fn with_an_unknown_icon_shows_the_server_one_and_saves_it() {
        let mut form = Form::editing(&Machine { icon: "🖥".into(), ..machine("box", "box.lan") });

        assert_eq!(form.icon.label(), Some("☁ server"));
        assert_eq!(form.checked(&[]).map(|(_, machine)| machine.icon), Some("☁".into()));
    }
}

mod refusals {
    use super::*;

    #[rstest]
    #[case::an_empty_name("", "x.lan", "~/code", Field::Name, "the name is required")]
    #[case::a_name_with_a_space(
        "my vps",
        "x.lan",
        "~/code",
        Field::Name,
        "use letters, digits, `.`, `_` and `-`, at most 20 characters"
    )]
    #[case::a_name_over_20_characters("a".repeat(21), "x.lan", "~/code", Field::Name, "use letters, digits, `.`, `_` and `-`, at most 20 characters")]
    #[case::a_name_in_use_in_another_case(
        "PI",
        "x.lan",
        "~/code",
        Field::Name,
        "another machine is already called `pi`"
    )]
    #[case::an_empty_destination("box", "", "~/code", Field::Destination, "the destination is required")]
    #[case::an_option_for_ssh(
        "box",
        "-oProxyCommand=sh",
        "~/code",
        Field::Destination,
        "a destination cannot start with `-`"
    )]
    #[case::a_space(
        "box",
        "me@x .lan",
        "~/code",
        Field::Destination,
        "a destination cannot hold spaces, control or invisible characters"
    )]
    #[case::a_control_character(
        "box",
        "x\u{7}.lan",
        "~/code",
        Field::Destination,
        "a destination cannot hold spaces, control or invisible characters"
    )]
    #[case::over_255_bytes("box", "é".repeat(128), "~/code", Field::Destination, "a destination is at most 255 bytes")]
    #[case::a_destination_in_use(
        "box",
        "pi.lan",
        "~/code",
        Field::Destination,
        "the machine `pi` already uses this destination"
    )]
    #[case::an_empty_folder("box", "x.lan", "", Field::Folder, "the folder is required")]
    #[case::a_relative_folder("box", "x.lan", "code", Field::Folder, "use an absolute path or one starting with ~/")]
    fn are_said_under_their_field(
        #[case] name: String,
        #[case] destination: String,
        #[case] folder: String,
        #[case] field: Field,
        #[case] message: &str,
    ) {
        let mut form = filled(&name, &destination, &folder);

        assert_eq!(form.checked(&listed()), None);
        assert_eq!((form.errors.clone(), form.focus), (vec![(field, message.to_string())], field));
    }

    #[test]
    fn of_several_fields_each_stay_under_their_own_and_the_first_takes_the_focus() {
        let mut form = filled("", "x.lan", "code");
        form.focus = Field::Destination;

        assert_eq!(form.checked(&listed()), None);
        assert_eq!(
            (form.error(Field::Name), form.error(Field::Folder), form.focus),
            (Some("the name is required"), Some("use an absolute path or one starting with ~/"), Field::Name)
        );
    }

    #[test]
    fn go_once_their_field_is_typed_in() {
        let mut form = filled("", "", "~/code");
        form.checked(&listed());

        press(&mut form, KeyCode::Char('v'));

        assert_eq!((form.error(Field::Name), form.error(Field::Destination).is_some()), (None, true));
    }
}

mod keys {
    use super::*;

    #[test]
    fn typing_past_a_field_s_width_scrolls_it_to_the_cursor() {
        let mut form = filled("vps", "", "~/code");
        form.focus = Field::Destination;

        for c in "me@a-very-long-host.example.com".chars() {
            form.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), 10, 8);
        }

        assert_eq!(form.destination.scroll(), 3);
    }

    #[test]
    fn tab_goes_through_the_fields_and_around() {
        let mut form = Form::new();
        let mut seen = Vec::new();
        for _ in 0..7 {
            press(&mut form, KeyCode::Tab);
            seen.push(form.focus);
        }
        press(&mut form, KeyCode::BackTab);

        let around = [Field::Command, Field::Shown, Field::SyncFlows, Field::Name];
        assert_eq!(seen[..3], [Field::Icon, Field::Destination, Field::Folder]);
        assert_eq!(seen[3..], around);
        assert_eq!(form.focus, Field::SyncFlows);
    }

    #[test]
    fn typing_goes_into_the_focused_field() {
        let mut form = Form::new();
        for c in "pi".chars() {
            press(&mut form, KeyCode::Char(c));
        }

        assert_eq!(form.name.text(), "pi");
    }

    #[test]
    fn enter_saves_from_a_field_and_opens_the_icons_on_the_icon() {
        let mut form = Form::new();
        assert_eq!(press(&mut form, KeyCode::Enter), Step::Submit);

        form.focus = Field::Icon;
        assert_eq!(press(&mut form, KeyCode::Enter), Step::None);
        assert!(form.icon.is_open());
        press(&mut form, KeyCode::Down);
        press(&mut form, KeyCode::Enter);

        assert_eq!((form.icon.label(), form.icon.is_open()), (Some("▭ laptop"), false));
    }

    #[test]
    fn esc_closes_the_icons_first_then_goes_back() {
        let mut form = Form { focus: Field::Icon, ..Form::new() };
        press(&mut form, KeyCode::Enter);

        assert_eq!(press(&mut form, KeyCode::Esc), Step::None);
        assert_eq!(press(&mut form, KeyCode::Esc), Step::Back);
    }

    #[test]
    fn a_paste_is_one_line_in_the_focused_field() {
        let mut form = Form { focus: Field::Destination, ..Form::new() };

        form.paste("me@vps\n.lan", WIDTH);

        assert_eq!(form.destination.text(), "me@vps .lan");
    }
}

mod clicks {
    use super::*;

    #[test]
    fn on_the_icon_open_its_list_and_a_choice_picks_one() {
        let mut form = Form::new();

        form.hit(Hit::Field { field: Field::Icon, line: 0, col: 0 }, WIDTH);
        assert_eq!((form.focus, form.icon.is_open()), (Field::Icon, true));
        form.hit(Hit::Choose(4), WIDTH);

        assert_eq!((form.icon.label(), form.icon.is_open()), (Some("⌂ home server"), false));
    }

    #[test]
    fn on_a_field_put_the_cursor_there() {
        let mut form = filled("vps", "me@vps.lan", "~/code");

        form.hit(Hit::Field { field: Field::Destination, line: 0, col: 2 }, WIDTH);

        assert_eq!((form.focus, form.destination.cursor()), (Field::Destination, 2));
    }

    #[test]
    fn on_a_scrolled_field_put_the_cursor_on_the_line_shown() {
        let mut form = filled("vps", "", "~/code");
        form.focus = Field::Destination;
        form.paste("me@a-very-long-host.example.com", 10);
        assert_eq!(form.destination.scroll(), 3);

        form.hit(Hit::Field { field: Field::Destination, line: 0, col: 0 }, 10);

        assert_eq!(form.destination.cursor(), 30);
    }

    #[rstest]
    #[case::back(BACK, Step::Back)]
    #[case::add(ADD, Step::Submit)]
    #[case::save(SAVE, Step::Submit)]
    fn on_a_button_do_what_it_says(#[case] label: &'static str, #[case] step: Step) {
        assert_eq!(Form::new().hit(Hit::Button(label), WIDTH), step);
    }
}

mod the_command_and_the_box {
    use super::*;

    fn on(field: Field) -> Form {
        Form { focus: field, ..filled("vps", "vps.lan", "~/code") }
    }

    #[test]
    fn a_new_form_shows_the_machine_with_no_command_and_goes_back_to_settings() {
        let form = Form::new();

        assert_eq!((form.shown, form.command.text(), form.back_to_settings), (true, String::new(), true));
        assert!(!Form::from_button().back_to_settings);
    }

    #[test]
    fn an_edited_machine_fills_both() {
        let hidden = Machine { command: Some("~/bin/cc".into()), shown: false, ..machine("vps", "vps.lan") };

        let form = Form::editing(&hidden);

        assert_eq!((form.command.text().as_str(), form.shown), ("~/bin/cc", false));
    }

    #[test]
    fn space_toggles_the_box_and_other_characters_do_nothing() {
        let mut form = on(Field::Shown);

        press(&mut form, KeyCode::Char('x'));
        assert!(form.shown);
        press(&mut form, KeyCode::Char(' '));
        assert!(!form.shown);
        press(&mut form, KeyCode::Char(' '));
        assert!(form.shown);
    }

    #[test]
    fn enter_on_the_box_saves() {
        assert_eq!(press(&mut on(Field::Shown), KeyCode::Enter), Step::Submit);
    }

    #[test]
    fn a_click_on_the_box_focuses_and_toggles_it() {
        let mut form = Form::new();

        form.hit(Hit::Field { field: Field::Shown, line: 0, col: 1 }, WIDTH);

        assert_eq!((form.focus, form.shown), (Field::Shown, false));
    }

    #[test]
    fn an_empty_command_is_built_as_none_and_the_box_as_it_is() {
        let mut form = on(Field::Command);
        form.shown = false;

        let (_, built) = form.checked(&[]).expect("valid");

        assert_eq!((built.command, built.shown), (None, false));
    }

    #[test]
    fn a_command_is_trimmed() {
        let mut form = filled("vps", "vps.lan", "~/code");
        form.command = Editor::new("  ~/bin/cc ");

        let (_, built) = form.checked(&[]).expect("valid");

        assert_eq!(built.command.as_deref(), Some("~/bin/cc"));
    }

    #[test]
    fn a_command_with_a_control_character_is_refused_under_its_field() {
        let mut form = filled("vps", "vps.lan", "~/code");
        form.command = Editor::new("cc\u{1b}[2J");

        assert_eq!(form.checked(&[]), None);

        assert_eq!(
            (form.error(Field::Command), form.focus),
            (Some("a command cannot hold control or invisible characters"), Field::Command)
        );
    }

    #[rstest]
    #[case::the_same_destination("vps.lan", true)]
    #[case::another_one("other.lan", false)]
    fn the_mark_of_a_program_survives_only_while_the_destination_does(#[case] destination: &str, #[case] kept: bool) {
        let marked = Machine { unreached: true, ..machine("vps", "vps.lan") };
        let mut form = Form::editing(&marked);
        form.destination = Editor::new(destination);

        let (_, built) = form.checked(&[marked]).expect("valid");

        assert_eq!(built.unreached, kept);
    }
}

mod syncing_flows {
    use super::*;

    #[test]
    fn is_off_for_a_new_machine_and_space_turns_it_on() {
        let mut form = filled("vps", "me@vps.lan", "~/code");
        form.focus = Field::SyncFlows;

        press(&mut form, KeyCode::Char(' '));
        press(&mut form, KeyCode::Char('x'));

        let (_, machine) = form.checked(&listed()).expect("valid");
        assert!(machine.sync_flows && machine.shown);
    }

    #[test]
    fn a_click_toggles_it_and_an_edit_keeps_what_the_machine_had() {
        let synced = Machine { sync_flows: true, ..machine("pi", "pi.lan") };
        let mut form = Form::editing(&synced);

        form.hit(Hit::Field { field: Field::SyncFlows, line: 0, col: 0 }, WIDTH);

        assert_eq!((form.focus, form.sync_flows, form.shown), (Field::SyncFlows, false, true));
        assert_eq!(press(&mut form, KeyCode::Enter), Step::Submit);
    }
}
