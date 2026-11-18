use super::*;
use crate::bots::asks::{Hit, LATER, NEVER, SAVE};
use crate::editor::Editor;
use crate::ui::bot_ask::{self, AskField, BotAsk, hit};

fn field(name: &str, typed: &str, replaces: bool) -> AskField {
    AskField { name: name.into(), value: Editor::new(typed).masked(), replaces }
}

fn ask(fields: Vec<AskField>) -> BotAsk {
    BotAsk {
        title: "Campus asks for a secret".into(),
        agent: "Claude Code".into(),
        access: "through cornercase".into(),
        full: false,
        about: "to read your grades on the campus site, which needs your university login".into(),
        fields,
        focus: 0,
        error: None,
    }
}

fn shown(ask: BotAsk) -> View<'static> {
    View { overlay: Some(Overlay::BotAsk(Box::new(ask))), ..view(&["~"]) }
}

fn text_of(t: &Terminal<TestBackend>, height: u16, width: u16) -> String {
    (0..height).map(|y| row_text(t, Rect::new(0, y, width, 1))).collect::<Vec<_>>().join("\n")
}

#[test]
fn one_name_is_drawn() {
    insta::assert_snapshot!(render(&shown(ask(vec![field("campus-password", "", false)]))).backend());
}

#[test]
fn four_names_are_drawn() {
    let names = ["campus-user", "campus-password", "mail-token", "pin"];
    let fields = names.iter().map(|name| field(name, "", false)).collect();
    insta::assert_snapshot!(render(&shown(ask(fields))).backend());
}

#[test]
fn a_name_that_replaces_a_saved_one_says_so() {
    let t = render(&shown(ask(vec![field("campus-password", "", true)])));
    assert!(text_of(&t, AREA.height, AREA.width).contains("campus-password replaces the saved one"));
}

#[test]
fn a_typed_value_is_drawn_as_bullets() {
    let t = render(&shown(ask(vec![field("pw", "hunter2-typed", false)])));
    let text = text_of(&t, AREA.height, AREA.width);
    assert!(text.contains("•••••••••••••") && !text.contains("hunter2"), "{text}");
}

#[test]
fn full_access_is_red() {
    let full = BotAsk { access: "full access".into(), full: true, ..ask(vec![field("pw", "", false)]) };
    let t = render(&shown(full));
    let found = (0..AREA.height).find_map(|y| {
        let text = row_text(&t, Rect::new(0, y, AREA.width, 1));
        let x = text.find("full access")?;
        Some(Position::new(u16::try_from(text[..x].chars().count()).ok()?, y))
    });
    assert_eq!(t.backend().buffer()[found.expect("the access")].fg, Color::Red);
}

#[test]
fn a_refusal_is_drawn_under_the_form() {
    let refused = BotAsk {
        error: Some("campus has 20 secrets, the most there can be".into()),
        ..ask(vec![field("pw", "x", false)])
    };
    insta::assert_snapshot!(render(&shown(refused)).backend());
}

#[test]
fn a_narrow_window_keeps_the_fields_and_the_buttons() {
    let fields =
        ["campus-user", "campus-password", "mail-token", "pin"].iter().map(|name| field(name, "", false)).collect();
    insta::assert_snapshot!(render_sized(&shown(ask(fields)), 60, 18).backend());
}

#[test]
fn every_button_and_field_is_found_where_it_is_drawn() {
    let view = ask(vec![field("user", "", false), field("pw", "", false)]);
    let t = render(&shown(ask(vec![field("user", "", false), field("pw", "", false)])));
    let text = text_of(&t, AREA.height, AREA.width);
    let (y, row) = text.lines().enumerate().find(|(_, row)| row.contains(" never ")).expect("the buttons");
    for label in ["never", "not now", "save"] {
        let x = row[..row.find(&format!(" {label} ")).expect(label)].chars().count() + 1;
        let pos = Position::new(u16::try_from(x).expect("x"), u16::try_from(y).expect("y"));
        let wanted = match label {
            "never" => NEVER,
            "not now" => LATER,
            _ => SAVE,
        };
        assert_eq!(hit(AREA, &view, pos), Some(Hit::Button(wanted)));
    }
    let second = bot_ask::text(AREA, &view, 1);
    assert_eq!(hit(AREA, &view, second.as_position()), Some(Hit::Field { at: 1, col: 0 }));
}
