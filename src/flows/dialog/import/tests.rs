use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rstest::rstest;

use super::super::tests::{ROOM, dialog, edit, editing};
use super::super::{Confirm, EXPORT, Place};
use super::*;
use crate::flows::share;

fn shared(name: &str) -> String {
    let mut flow = dialog().flows[0].flow.clone();
    flow.name = name.into();
    share::to_share(&flow).expect("export")
}

fn press(d: &mut Dialog, label: &str) -> Action {
    d.press(label)
}

mod the_buttons {
    use super::*;

    #[test]
    fn export_shows_only_for_a_flow_that_was_read() {
        let mut d = dialog();
        assert!(d.buttons(ROOM).contains(&EXPORT));
        d.select(d.flows.len());
        assert!(!d.buttons(ROOM).contains(&EXPORT), "{:?}", d.buttons(ROOM));
        assert!(d.buttons(ROOM).contains(&IMPORT));
    }

    #[rstest]
    #[case::roomy(90, &["edit", "duplicate", "set default", "export", "delete", "diagram", "import", "new", "done"])]
    #[case::tight(73, &["edit", "duplicate", "set default", "export", "delete", "diagram", "new", "done"])]
    #[case::tighter(64, &["edit", "duplicate", "set default", "delete", "diagram", "new", "done"])]
    #[case::tightest(50, &["edit", "duplicate", "delete", "diagram", "new", "done"])]
    fn narrow_rooms_drop_import_then_export_then_set_default(#[case] width: usize, #[case] expected: &[&str]) {
        let mut d = dialog();
        d.select(4);
        let room = Room { width, wide: false, ..ROOM };
        assert_eq!(d.buttons(room), expected);
    }

    #[test]
    fn export_copies_the_selected_flow() {
        let mut d = dialog();
        d.select(4);
        let Action::Export(flow) = press(&mut d, EXPORT) else { panic!("export") };
        assert_eq!(flow.name, "Mine");
    }
}

mod the_page {
    use super::*;

    #[test]
    fn import_opens_the_page_and_escape_goes_back() {
        let mut d = dialog();
        press(&mut d, IMPORT);
        assert!(d.importing.is_some());
        assert_eq!(d.title(), "Flows › import");
        assert_eq!(d.buttons(ROOM), [IMPORT, CANCEL]);
        d.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), ROOM);
        assert!(d.importing.is_none());
        press(&mut d, IMPORT);
        press(&mut d, CANCEL);
        assert!(d.importing.is_none());
    }

    #[test]
    fn a_paste_on_the_list_opens_the_page_with_the_text() {
        let mut d = dialog();
        d.paste("name = \"x\"\n", ROOM);
        let importing = d.importing.as_ref().expect("the page");
        assert_eq!(importing.editor.text(), "name = \"x\"\n");
    }

    #[test]
    fn bad_text_sets_the_error_and_keeps_the_page() {
        let mut d = dialog();
        d.paste("name = = 1", ROOM);
        press(&mut d, IMPORT);
        let error = d.error().expect("an error");
        assert!(error.starts_with("line 1: "), "{error}");
        assert!(d.importing.is_some() && d.edit.is_none());
    }

    #[test]
    fn good_text_opens_the_editor_unsaved_with_a_name_of_its_own() {
        let mut d = dialog();
        d.paste(&shared("quick"), ROOM);
        press(&mut d, IMPORT);
        assert!(d.importing.is_none());
        let imported = edit(&d);
        assert_eq!(imported.flow.name, "quick 2");
        assert!(imported.dirty() && imported.path.is_none() && imported.place == Place::Yours);
        d.back();
        assert!(matches!(d.confirm, Some(Confirm::Leave(_))));
    }

    #[test]
    fn an_imported_flow_that_sets_variables_gets_the_red_warning() {
        let mut flow = dialog().flows[0].flow.clone();
        flow.roles[0].env.insert("TOKEN".into(), "x".into());
        let mut d = dialog();
        d.paste(&share::to_share(&flow).expect("export"), ROOM);
        press(&mut d, IMPORT);
        assert!(d.warnings().iter().any(|w| w.starts_with("this imported flow sets TOKEN")), "{:?}", d.warnings());
        let mut plain = dialog();
        plain.paste(&shared("Fresh"), ROOM);
        press(&mut plain, IMPORT);
        assert!(plain.warnings().iter().all(|w| !w.contains("imported")), "{:?}", plain.warnings());
    }

    #[test]
    fn agents_that_are_not_installed_are_named() {
        let mut flow = dialog().flows[0].flow.clone();
        flow.roles[0].agent = "grok".into();
        let mut d = dialog();
        d.paste(&share::to_share(&flow).expect("export"), ROOM);
        press(&mut d, IMPORT);
        let notice = edit(&d).notice.clone().expect("a notice");
        assert!(notice.contains("grok, which is not installed here"), "{notice}");
    }

    #[test]
    fn a_name_with_a_control_character_keeps_the_page_open_with_the_error() {
        let mut d = dialog();
        d.paste(&shared("x\x1b[2J"), ROOM);
        press(&mut d, IMPORT);
        assert!(d.importing.is_some() && d.edit.is_none());
        let error = d.error().expect("an error");
        assert!(error.contains("control character") && !error.contains('\x1b'), "{error:?}");
        assert!(!d.title().contains('\x1b'));
    }

    #[test]
    fn a_paste_while_a_confirmation_is_pending_is_ignored() {
        let mut d = dialog();
        d.select(4);
        d.ask_delete();
        d.paste("name = \"x\"", ROOM);
        assert!(d.confirm.is_some() && d.importing.is_none());
    }

    #[test]
    fn a_paste_past_the_cap_is_refused() {
        let mut d = dialog();
        d.paste(&"x".repeat(share::MAX_BYTES), ROOM);
        d.paste("more", ROOM);
        assert_eq!(d.error(), Some(share::too_large()));
        assert_eq!(d.importing.as_ref().expect("page").editor.text().len(), share::MAX_BYTES);
    }

    #[test]
    fn a_flow_opened_for_editing_is_not_an_import() {
        let d = editing();
        assert!(!edit(&d).imported && d.warnings().iter().all(|w| !w.contains("imported")));
    }
}

mod fetching {
    use super::*;
    use crate::flows::registry::Source;

    fn typed(d: &mut Dialog, text: &str) {
        d.paste(text, ROOM);
    }

    fn repo() -> Source {
        Source::Repo("octocat/fresh".into())
    }

    #[test]
    fn a_repo_or_an_address_starts_a_fetch_and_waits() {
        let mut d = dialog();
        typed(&mut d, "octocat/fresh");
        assert!(matches!(press(&mut d, IMPORT), Action::Fetch(source) if source == repo()));
        assert_eq!(d.hint(), format!("fetching {}…", repo().address()));
        assert!(d.hint().contains("/octocat/fresh/HEAD/flow.toml"), "{}", d.hint());
        assert!(matches!(press(&mut d, IMPORT), Action::None), "a second press waits");
        let mut d = dialog();
        typed(&mut d, "https://example.com/fresh.toml");
        assert!(matches!(press(&mut d, IMPORT), Action::Fetch(Source::Url(_))));
    }

    #[test]
    fn the_answer_opens_the_editor_unsaved_with_a_notice() {
        let mut d = dialog();
        typed(&mut d, "octocat/fresh");
        press(&mut d, IMPORT);
        d.fetched(&repo(), Ok(shared("Fresh")));
        assert!(d.importing.is_none());
        let imported = edit(&d);
        assert_eq!(imported.flow.name, "Fresh");
        assert!(imported.dirty() && imported.path.is_none());
        let notice = imported.notice.clone().expect("a notice");
        assert!(notice.contains("from octocat/fresh: nobody reviewed it, read its prompts before saving"), "{notice}");
    }

    #[test]
    fn a_failure_stays_on_the_page() {
        let mut d = dialog();
        typed(&mut d, "octocat/fresh");
        press(&mut d, IMPORT);
        d.fetched(&repo(), Err("cannot fetch it".into()));
        assert_eq!(d.error().as_deref(), Some("cannot fetch it"));
        assert!(d.importing.as_ref().is_some_and(|i| i.fetching.is_none()));
    }

    #[test]
    fn an_answer_for_another_source_or_a_closed_page_is_ignored() {
        let mut d = dialog();
        typed(&mut d, "octocat/fresh");
        press(&mut d, IMPORT);
        d.fetched(&Source::Repo("octocat/other".into()), Ok(shared("Other")));
        assert!(d.importing.is_some() && d.edit.is_none());
        d.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), ROOM);
        d.fetched(&repo(), Ok(shared("Fresh")));
        assert!(d.edit.is_none());
    }

    #[test]
    fn text_with_lines_is_never_fetched() {
        let mut d = dialog();
        typed(&mut d, "octocat/flow\nname = = 1");
        assert!(matches!(press(&mut d, IMPORT), Action::None));
        assert!(d.importing.as_ref().is_some_and(|i| i.fetching.is_none()));
    }
}
