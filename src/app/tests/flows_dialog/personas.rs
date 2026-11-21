use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::*;
use crate::flows::dialog::Page;
use crate::flows::dialog::personas::catalogue::IMPORT_SELECTED;
use crate::flows::dialog::personas::{PERSONAS, REMOVE};
use crate::flows::persona::source::Hosts;
use crate::flows::persona::{self, draft};
use crate::test_util::FakeHttp;

const REVIEWER: &str = "---\nname: Code Reviewer\ndescription: Reviews code.\nemoji: 👁️\n---\n\nYou review code.\n";
const TESTER: &str = "---\nname: Tester\ndescription: Tests it.\n---\n\nYou test.\n";
const DIVISIONS: &str = r#"{"divisions": {"engineering": {"label": "Engineering"}, "testing": {"label": "Testing"}}}"#;

fn library(s: &Setup) -> PathBuf {
    paths::personas(&s.config.path().join("config.json"))
}

fn page(app: &App) -> &Page {
    dialog(app).personas.as_deref().expect("the personas page is open")
}

fn personas_page() -> Setup {
    let mut s = open(false);
    button(&mut s.app, WIDE, PERSONAS);
    s
}

fn with_reviewer() -> Setup {
    let s = setup(false);
    std::fs::create_dir_all(library(&s)).expect("the folder");
    std::fs::write(library(&s).join("reviewer.md"), REVIEWER).expect("write");
    let mut s = s;
    s.app.open_flows(None, None);
    button(&mut s.app, WIDE, PERSONAS);
    s
}

fn fill(s: &mut Setup, row: usize, text: &str) {
    click_list_row(&mut s.app, WIDE, row);
    type_in(&mut s.app, text, WIDE);
    key_in(&mut s.app, KeyCode::Enter, WIDE);
}

fn archive_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (path, text) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, format!("agents-HEAD/{path}"), text.as_bytes()).expect("append");
    }
    builder.into_inner().expect("tar").finish().expect("gzip")
}

fn fetched(s: &mut Setup) {
    while page(&s.app).catalogue.as_ref().is_some_and(|catalogue| catalogue.fetching.is_some()) {
        let event = s.rx.recv_timeout(Duration::from_secs(10)).expect("the fetch answers");
        s.app.handle_event(event, WIDE).expect("handle");
    }
}

#[test]
fn an_empty_library_says_how_to_get_personas() {
    let mut s = personas_page();
    let screen = shown(&mut s.app, WIDE);
    assert!(screen.contains("Flows › personas"), "{screen}");
    assert!(screen.contains("no personas yet"), "{screen}");
}

#[test]
fn new_writes_a_persona_of_yours_from_its_fields() {
    let mut s = personas_page();
    button(&mut s.app, WIDE, dialog::NEW);
    fill(&mut s, 0, "Careful Tester");
    fill(&mut s, 2, "🧪");
    click_list_row(&mut s.app, WIDE, 3);
    type_in(&mut s.app, "Write a test first.", WIDE);
    button(&mut s.app, WIDE, dialog::SAVE);
    button(&mut s.app, WIDE, dialog::SAVE);

    let file = std::fs::read_to_string(library(&s).join("careful-tester.md")).expect("saved");
    assert_eq!(draft::body(&file), "Write a test first.\n");
    assert!(!file.contains("source:"), "{file}");
    assert!(page(&s.app).form.is_none());
    assert_eq!(page(&s.app).library[0].emoji, "🧪");
    assert!(dialog(&s.app).agents.personas.iter().any(|named| named.id == "careful-tester"));
}

#[test]
fn a_new_persona_without_instructions_is_not_saved() {
    let mut s = personas_page();
    button(&mut s.app, WIDE, dialog::NEW);
    fill(&mut s, 0, "Empty");
    button(&mut s.app, WIDE, dialog::SAVE);

    assert!(shown(&mut s.app, WIDE).contains("write its instructions"));
    assert!(!library(&s).join("empty.md").exists());
}

#[test]
fn edit_keeps_the_id_and_changes_the_file() {
    let mut s = with_reviewer();
    click_list_row(&mut s.app, WIDE, 0);
    click_list_row(&mut s.app, WIDE, 1);
    for _ in 0.."Reviews code.".len() {
        key_in(&mut s.app, KeyCode::Backspace, WIDE);
    }
    type_in(&mut s.app, "Reviews twice.", WIDE);
    key_in(&mut s.app, KeyCode::Enter, WIDE);
    button(&mut s.app, WIDE, dialog::SAVE);

    let file = std::fs::read_to_string(library(&s).join("reviewer.md")).expect("kept");
    assert_eq!(persona::parse("reviewer", &file).expect("a persona").description, "Reviews twice.");
    assert_eq!(draft::body(&file), "You review code.\n");
}

#[test]
fn remove_asks_naming_the_flows_that_follow_it() {
    let mut s = with_reviewer();
    let mut flow = dialog(&s.app).flows[0].flow.clone();
    flow.name = "Reviewed".into();
    flow.roles[0].persona = Some("reviewer".into());
    store::save(&user_dir(&s), &flow, None).expect("a flow");
    s.app.refresh_flows();
    s.app.refresh_personas_page();

    button(&mut s.app, WIDE, REMOVE);
    let question = shown(&mut s.app, WIDE);
    assert!(question.contains("Reviewed follow it"), "{question}");
    key_in(&mut s.app, KeyCode::Enter, WIDE);

    assert!(!library(&s).join("reviewer.md").exists());
    assert_eq!(page(&s.app).library, Vec::new());
}

#[test]
fn import_fetches_a_repository_by_division_and_imports_only_the_checked_ones() {
    let archive = archive_of(&[
        ("divisions.json", DIVISIONS),
        ("engineering/reviewer.md", REVIEWER),
        ("testing/tester.md", TESTER),
    ]);
    let server = FakeHttp::start(vec![("GET /octo/agents/tar.gz/HEAD", 200, archive)]);
    let mut s = personas_page();
    s.app.runs.persona_hosts = Hosts { raw: server.url(), archive: server.url() };
    button(&mut s.app, WIDE, dialog::IMPORT);
    type_in(&mut s.app, "octo/agents", WIDE);
    key_in(&mut s.app, KeyCode::Enter, WIDE);
    fetched(&mut s);

    let screen = shown(&mut s.app, WIDE);
    assert!(screen.contains("[ ] Engineering") && screen.contains("[ ] Testing"), "{screen}");
    assert!(screen.contains("from octo/agents, which nobody here reviewed"), "{screen}");
    click_list_row(&mut s.app, WIDE, 0);
    button(&mut s.app, WIDE, IMPORT_SELECTED);
    assert!(shown(&mut s.app, WIDE).contains("Import 1 persona from octo/agents?"));
    key_in(&mut s.app, KeyCode::Enter, WIDE);

    assert!(library(&s).join("reviewer.md").exists());
    assert!(!library(&s).join("tester.md").exists());
    assert_eq!(page(&s.app).notice.as_deref(), Some("imported 1 persona"));
}

#[test]
fn a_fetch_that_fails_says_why_and_keeps_the_source() {
    let server = FakeHttp::start(Vec::<(&str, u16, &str)>::new());
    let mut s = personas_page();
    s.app.runs.persona_hosts = Hosts { raw: server.url(), archive: server.url() };
    button(&mut s.app, WIDE, dialog::IMPORT);
    type_in(&mut s.app, "octo/none", WIDE);
    key_in(&mut s.app, KeyCode::Enter, WIDE);
    fetched(&mut s);

    let catalogue = page(&s.app).catalogue.as_ref().expect("the import page");
    assert_eq!(catalogue.source, "octo/none");
    assert!(catalogue.error.as_deref().is_some_and(|e| e.contains("no repository octo/none")), "{:?}", catalogue.error);
}

#[test]
fn a_role_picks_a_persona_and_the_flow_saves_it() {
    let mut s = with_reviewer();
    button(&mut s.app, WIDE, dialog::DONE);
    duplicate_full_issue(&mut s, WIDE);
    tab(&mut s.app, WIDE, 3);
    click_row(&mut s.app, WIDE, Row::Persona);
    let pick = view(&s.app, WIDE).pick.expect("the persona pick");
    let labels: Vec<&str> = pick.items.iter().map(|(label, _, _)| label.as_str()).collect();
    assert_eq!(labels, ["none", "👁️ Code Reviewer"]);
    key_in(&mut s.app, KeyCode::Down, WIDE);
    key_in(&mut s.app, KeyCode::Enter, WIDE);
    button(&mut s.app, WIDE, dialog::SAVE);

    let saved = std::fs::read_to_string(user_dir(&s).join("full-issue-copy.toml")).expect("the flow");
    assert!(saved.contains("persona = \"reviewer\""), "{saved}");
}

mod pages {
    use super::*;

    #[test]
    fn the_library() {
        let mut s = with_reviewer();
        insta::assert_snapshot!(shown(&mut s.app, WIDE));
    }

    #[test]
    fn the_form() {
        let mut s = with_reviewer();
        click_list_row(&mut s.app, WIDE, 0);
        insta::assert_snapshot!(shown(&mut s.app, WIDE));
    }

    #[test]
    fn the_catalogue() {
        let archive = archive_of(&[
            ("divisions.json", DIVISIONS),
            ("engineering/reviewer.md", REVIEWER),
            ("testing/tester.md", TESTER),
        ]);
        let server = FakeHttp::start(vec![("GET /octo/agents/tar.gz/HEAD", 200, archive)]);
        let mut s = with_reviewer();
        s.app.runs.persona_hosts = Hosts { raw: server.url(), archive: server.url() };
        button(&mut s.app, WIDE, dialog::IMPORT);
        type_in(&mut s.app, "octo/agents", WIDE);
        key_in(&mut s.app, KeyCode::Enter, WIDE);
        fetched(&mut s);
        insta::assert_snapshot!(shown(&mut s.app, WIDE).replace(&server.url(), "SERVER"));
    }
}
