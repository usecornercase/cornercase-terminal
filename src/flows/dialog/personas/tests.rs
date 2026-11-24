use std::collections::BTreeMap;

use crossterm::event::KeyModifiers;

use super::*;
use crate::flows::persona::source::Division;
use crate::flows::{ORCHESTRATOR, Role};

fn persona(id: &str, name: &str) -> Persona {
    Persona {
        id: id.into(),
        name: name.into(),
        text: format!("---\nname: {name}\n---\n\nBody.\n"),
        ..Persona::default()
    }
}

fn page() -> Page {
    let listing = Listing { personas: vec![persona("reviewer", "Code Reviewer")], broken: Vec::new() };
    Page::new(listing, BTreeMap::new())
}

fn offered(name: &str, division: &str) -> Offered {
    Offered { name: name.into(), id: name.to_lowercase(), division: Some(division.into()), ..Offered::default() }
}

fn catalogue() -> Catalogue {
    let mut catalogue = Catalogue { fetching: Some("octo/agents".into()), ..Catalogue::default() };
    let fetched = Fetched {
        label: "octo/agents".into(),
        web: true,
        divisions: vec![Division {
            key: "eng".into(),
            label: "Engineering".into(),
            icon: Some("Code".into()),
            color: Some("#3B82F6".into()),
        }],
        offered: vec![offered("Ada", "eng"), offered("Bob", "eng"), offered("Cy", "ops")],
        ..Fetched::default()
    };
    catalogue.loaded("octo/agents", Ok(fetched));
    catalogue
}

#[test]
fn followers_name_each_flow_once_per_persona() {
    let role = |name: &str, persona: Option<&str>| Role {
        name: name.into(),
        persona: persona.map(str::to_string),
        ..Role::default()
    };
    let flow = Flow {
        name: "Reviewed".into(),
        orchestrator: role(ORCHESTRATOR, Some("reviewer")),
        roles: vec![role("a", Some("reviewer")), role("b", None)],
        ..Flow::default()
    };

    assert_eq!(followers(&[flow]), BTreeMap::from([("reviewer".to_string(), vec!["Reviewed".to_string()])]));
}

#[test]
fn removing_asks_first_and_names_its_flows() {
    let mut page = page();
    page.followers = BTreeMap::from([("reviewer".into(), vec!["Reviewed".into()])]);

    assert_eq!(page.press(REMOVE), PageAction::None);
    let (_, question, _) = page.ask_text().expect("a question");

    assert!(question.contains("Reviewed follow it"), "{question}");
    assert_eq!(page.answered(true), PageAction::Remove("reviewer".into()));
}

#[test]
fn a_division_row_checks_all_its_personas_and_a_second_click_unchecks_them() {
    let mut catalogue = catalogue();
    let labels: Vec<String> = catalogue.rows().into_iter().map(|row| row.label).collect();
    assert_eq!(labels, ["[ ] Engineering", "  [ ] Ada", "  [ ] Bob", "[ ] ops", "  [ ] Cy"]);

    catalogue.toggle(0);
    assert_eq!(catalogue.checked, [true, true, false]);
    catalogue.toggle(0);
    assert_eq!(catalogue.checked, [false, false, false]);
}

#[test]
fn import_selected_asks_with_the_risk_and_imports_only_what_is_checked() {
    let mut page = page();
    page.catalogue = Some(Box::new(catalogue()));
    assert_eq!(page.press(catalogue::IMPORT_SELECTED), PageAction::None);
    assert_eq!(page.error().as_deref(), Some("check the personas to import"));

    page.activate(4);
    page.press(catalogue::IMPORT_SELECTED);
    let (_, question, _) = page.ask_text().expect("a question");

    assert!(question.starts_with("Import 1 persona from octo/agents?"), "{question}");
    assert!(question.contains("Nobody here reviewed them"), "{question}");
    assert!(matches!(page.answered(true), PageAction::Import(chosen) if chosen.len() == 1 && chosen[0].name == "Cy"));
}

#[test]
fn an_answer_for_another_source_is_dropped() {
    let mut catalogue = Catalogue { fetching: Some("octo/new".into()), ..Catalogue::default() };
    catalogue.loaded("octo/old", Err("late".into()));
    assert_eq!((catalogue.fetching.as_deref(), catalogue.error), (Some("octo/new"), None));
}

#[test]
fn leaving_a_changed_form_asks_first() {
    let mut page = page();
    page.press(EDIT);
    if let Some(form) = &mut page.form {
        form.draft.description = "changed".into();
    }

    page.press(CANCEL);

    assert_eq!(page.ask, Some(Ask::Leave));
    page.answered(true);
    assert!(page.form.is_none());
}

mod the_catalogue_filter {
    use super::*;

    const ROOM: Room = Room { pick: 10, width: 40, height: 5, wide: true };

    fn typed(catalogue: &mut Catalogue, text: &str) {
        for c in text.chars() {
            catalogue.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), ROOM);
        }
    }

    fn labels(catalogue: &Catalogue) -> Vec<String> {
        catalogue.rows().into_iter().map(|row| row.label).collect()
    }

    #[test]
    fn shows_a_division_with_only_its_matching_personas() {
        let mut catalogue = catalogue();

        typed(&mut catalogue, "bo");

        assert_eq!(labels(&catalogue), ["[ ] Engineering", "  [ ] Bob"]);
        assert!(catalogue.heading().starts_with("1 of 3 in octo/agents match"), "{}", catalogue.heading());
    }

    #[test]
    fn matches_the_division_label_the_id_and_the_description() {
        let mut catalogue = catalogue();
        if let Some(fetched) = &mut catalogue.fetched {
            fetched.offered[2].description = "Watches the pager".into();
        }

        typed(&mut catalogue, "engineer");
        assert_eq!(labels(&catalogue), ["[ ] Engineering", "  [ ] Ada", "  [ ] Bob"]);
        catalogue.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), ROOM);
        typed(&mut catalogue, "pager");
        assert_eq!(labels(&catalogue), ["[ ] ops", "  [ ] Cy"]);
    }

    #[test]
    fn takes_every_word_and_a_space_types_instead_of_checking() {
        let mut catalogue = catalogue();

        typed(&mut catalogue, "eng ada");

        assert_eq!(labels(&catalogue), ["[ ] Engineering", "  [ ] Ada"]);
        assert_eq!(catalogue.checked, [false, false, false]);
    }

    #[test]
    fn checks_only_the_shown_personas_of_a_division_and_keeps_the_others_checked() {
        let mut catalogue = catalogue();
        catalogue.toggle(2);
        typed(&mut catalogue, "ada");

        catalogue.toggle(0);
        assert_eq!(catalogue.checked, [true, true, false]);
        catalogue.toggle(0);

        assert_eq!(catalogue.checked, [false, true, false]);
    }

    #[test]
    fn clears_on_the_first_esc_and_closes_on_the_second() {
        let mut catalogue = catalogue();
        typed(&mut catalogue, "zzz");
        assert_eq!(labels(&catalogue), Vec::<String>::new());

        assert!(matches!(
            catalogue.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), ROOM),
            catalogue::Pressed::Stay
        ));
        assert_eq!(labels(&catalogue).len(), 5);
        assert!(matches!(
            catalogue.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), ROOM),
            catalogue::Pressed::Close
        ));
    }

    #[test]
    fn gives_the_division_row_its_icon_and_colour() {
        let catalogue = catalogue();

        let row = catalogue.rows().into_iter().next().expect("the division");

        assert_eq!((row.icon.as_deref(), row.tint.as_deref()), (Some("Code"), Some("#3B82F6")));
    }
}
