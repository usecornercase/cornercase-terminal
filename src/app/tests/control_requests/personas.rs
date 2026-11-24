use super::*;
use crate::control::persona::{PersonaImport, PersonaList, PersonaRemove, PersonaText, Personas, PersonasImported};
use crate::control::{FlowImport, FlowList, FlowSave, FlowSpec, Flows};
use crate::flows::persona;
use crate::flows::{Flow, ORCHESTRATOR, Role, Stage, share};
use crate::paths;

const REVIEWER: &str = "---\nname: Code Reviewer\ndescription: Reviews code.\nemoji: 👁️\n---\n\nYou review code.\n";

struct Library {
    app: App,
    _rx: Receiver<AppEvent>,
    config: TempDir,
}

fn text(file: &str, body: &str) -> PersonaText {
    PersonaText { file: file.into(), text: body.into(), source: Some(format!("octo/agents/{file}")) }
}

fn following(id: &str) -> Flow {
    let role = |name: &str| Role { name: name.into(), agent: "claude".into(), ..Role::default() };
    Flow {
        name: "Reviewed".into(),
        orchestrator: role(ORCHESTRATOR),
        roles: vec![Role { persona: Some(id.into()), ..role("reviewer") }],
        stages: vec![Stage { roles: vec!["reviewer".into()], back: None }],
        ..Flow::default()
    }
}

impl Library {
    fn new() -> Self {
        let config = TempDir::new();
        let (app, rx) = new_app(write_config(config.path(), &Config::default()));
        Self { app, _rx: rx, config }
    }

    fn dir(&self) -> PathBuf {
        paths::personas(&self.config.path().join("config.json"))
    }

    fn import(&mut self, texts: Vec<PersonaText>, tweak: impl FnOnce(&mut PersonaImport)) -> Response {
        let mut import = PersonaImport { personas: texts, ..PersonaImport::default() };
        tweak(&mut import);
        now(&mut self.app, None, Command::PersonaImport(import))
    }

    fn imported(&mut self, texts: Vec<PersonaText>, tweak: impl FnOnce(&mut PersonaImport)) -> PersonasImported {
        match self.import(texts, tweak) {
            Response::Ok(value) => serde_json::from_value(value).expect("an import"),
            Response::Error(message) => panic!("not imported: {message}"),
        }
    }

    fn refused(&mut self, texts: Vec<PersonaText>, tweak: impl FnOnce(&mut PersonaImport)) -> String {
        match self.import(texts, tweak) {
            Response::Error(message) => message,
            Response::Ok(value) => panic!("imported: {value}"),
        }
    }

    fn listed(&mut self) -> Personas {
        serde_json::from_value(value(&mut self.app, None, Command::PersonaList(PersonaList::default())))
            .expect("a list")
    }
}

#[test]
fn an_import_saves_each_persona_by_its_id_with_its_source_and_lists_it() {
    let mut library = Library::new();

    let imported = library.imported(vec![text("engineering/engineering-code-reviewer.md", REVIEWER)], |_| {});

    assert!(imported.saved);
    assert_eq!(imported.personas[0].id, "engineering-code-reviewer");
    let path = library.dir().join("engineering-code-reviewer.md");
    assert_eq!(imported.personas[0].path.as_deref(), Some(path.as_path()));
    let saved = std::fs::read_to_string(&path).expect("the file");
    assert!(
        saved.contains("emoji: 👁️\nsource: \"octo/agents/engineering/engineering-code-reviewer.md\"\n---\n"),
        "{saved}"
    );
    let listed = library.listed();
    assert_eq!(listed.dir, library.dir());
    assert_eq!(listed.personas.len(), 1);
    assert_eq!((listed.personas[0].name.as_str(), listed.personas[0].emoji.as_str()), ("Code Reviewer", "👁️"));
    assert_eq!(listed.personas[0].source.as_deref(), Some("octo/agents/engineering/engineering-code-reviewer.md"));
}

#[test]
fn a_check_previews_and_saves_nothing() {
    let mut library = Library::new();

    let preview = library.imported(vec![text("reviewer.md", REVIEWER)], |import| import.check = true);

    assert!(!preview.saved);
    assert_eq!(preview.personas[0].path, None);
    assert!(preview.check.contains("without asking for permission"), "{}", preview.check);
    assert!(!library.dir().exists());
}

#[test]
fn a_persona_you_have_is_refused_unless_replaced() {
    let mut library = Library::new();
    library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});
    let newer = REVIEWER.replace("You review code.", "You review code twice.");

    let refused = library.refused(vec![text("reviewer.md", &newer)], |_| {});
    let replaced = library.imported(vec![text("reviewer.md", &newer)], |import| import.replace = true);

    assert!(refused.contains("you already have the persona `reviewer`: pass --replace"), "{refused}");
    assert!(replaced.personas[0].replaced);
    let saved = std::fs::read_to_string(library.dir().join("reviewer.md")).expect("the file");
    assert!(saved.contains("twice"), "{saved}");
}

#[test]
fn another_id_with_a_name_you_have_is_refused_even_with_replace() {
    let mut library = Library::new();
    library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});

    let refused = library.refused(vec![text("code-reviewer.md", REVIEWER)], |import| import.replace = true);

    assert!(refused.contains("your persona `reviewer` is also named `Code Reviewer`"), "{refused}");
    assert!(!library.dir().join("code-reviewer.md").exists());
}

#[test]
fn an_import_with_one_bad_persona_saves_none_and_names_it() {
    let mut library = Library::new();

    let refused = library.refused(
        vec![text("reviewer.md", REVIEWER), text("broken.md", "no front matter"), text("x/reviewer.md", REVIEWER)],
        |_| {},
    );

    assert!(refused.contains("`broken.md`: it has no front matter"), "{refused}");
    assert!(refused.contains("two of them would be the persona `reviewer`"), "{refused}");
    assert!(!library.dir().exists());
    assert!(library.refused(Vec::new(), |_| {}).contains("nothing to import"));
}

#[test]
fn two_personas_of_one_name_in_an_import_are_refused() {
    let mut library = Library::new();

    let refused = library.refused(vec![text("reviewer-a.md", REVIEWER), text("reviewer-b.md", REVIEWER)], |_| {});

    assert!(refused.contains("two of them are named `Code Reviewer`"), "{refused}");
    assert!(!library.dir().exists());
}

#[test]
fn a_file_of_that_id_that_cannot_be_read_is_kept_unless_replaced() {
    let mut library = Library::new();
    std::fs::create_dir_all(library.dir()).expect("the folder");
    std::fs::write(library.dir().join("reviewer.md"), "my notes").expect("write");

    let refused = library.refused(vec![text("reviewer.md", REVIEWER)], |_| {});

    assert!(refused.contains("you already have the persona `reviewer`"), "{refused}");
    assert_eq!(std::fs::read_to_string(library.dir().join("reviewer.md")).expect("kept"), "my notes");
}

#[test]
fn an_oversized_file_of_that_id_is_never_read_whole_nor_replaced() {
    let mut library = Library::new();
    std::fs::create_dir_all(library.dir()).expect("the folder");
    let big = "a".repeat(persona::MAX_BYTES + 1);
    std::fs::write(library.dir().join("reviewer.md"), &big).expect("write");

    let refused = library.refused(vec![text("reviewer.md", REVIEWER)], |import| import.replace = true);

    assert!(refused.contains("too large to keep a copy of"), "{refused}");
    assert_eq!(std::fs::read_to_string(library.dir().join("reviewer.md")).expect("kept").len(), big.len());
}

#[test]
fn an_import_that_cannot_save_one_persona_leaves_the_library_as_it_was() {
    let mut library = Library::new();
    library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});
    let before = std::fs::read_to_string(library.dir().join("reviewer.md")).expect("the file");
    std::fs::create_dir_all(library.dir().join("tester.md")).expect("a folder in the way");
    let newer = REVIEWER.replace("You review code.", "You review code twice.");
    let tester = "---\nname: Tester\n---\nTest it.\n";
    let fresh = "---\nname: Fresh\n---\nNew.\n";

    let refused = library.refused(
        vec![text("reviewer.md", &newer), text("fresh.md", fresh), text("tester.md", tester)],
        |import| {
            import.replace = true;
        },
    );

    assert!(refused.contains("could not save the persona `tester`"), "{refused}");
    assert_eq!(std::fs::read_to_string(library.dir().join("reviewer.md")).expect("restored"), before);
    assert!(!library.dir().join("fresh.md").exists());
}

#[test]
fn removing_a_persona_deletes_its_file() {
    let mut library = Library::new();
    library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});

    let removed = value(&mut library.app, None, Command::PersonaRemove(PersonaRemove { id: "reviewer".into() }));
    let again = now(&mut library.app, None, Command::PersonaRemove(PersonaRemove { id: "reviewer".into() }));

    assert_eq!((removed["id"].as_str(), removed["name"].as_str()), (Some("reviewer"), Some("Code Reviewer")));
    assert!(!library.dir().join("reviewer.md").exists());
    assert!(matches!(again, Response::Error(message) if message.contains("you have no persona `reviewer`")));
}

#[test]
fn a_flow_that_follows_a_persona_you_lack_cannot_run_until_you_import_it() {
    let mut library = Library::new();
    let flows = paths::flows(&library.config.path().join("config.json"));
    crate::flows::store::save(&flows, &following("reviewer"), None).expect("save the flow");
    let problems = |library: &mut Library| {
        let listed: Flows =
            serde_json::from_value(value(&mut library.app, None, Command::FlowList(FlowList::default())))
                .expect("flows");
        listed.flows.into_iter().find(|info| info.name == "Reviewed").expect("the flow").problems
    };

    let before = problems(&mut library);
    library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});
    let after = problems(&mut library);

    assert_eq!(before.len(), 1, "{before:?}");
    assert!(before[0].starts_with("`reviewer` follows the persona `reviewer`, which is not among your personas"));
    assert_eq!(after, Vec::<String>::new());
}

#[test]
fn a_flow_with_a_persona_you_lack_is_neither_imported_nor_saved() {
    let mut library = Library::new();
    let flow = following("reviewer");
    let shared = share::to_share(&flow).expect("shared");

    let imported =
        now(&mut library.app, None, Command::FlowImport(FlowImport { text: shared, ..FlowImport::default() }));
    let save = FlowSave { flow: FlowSpec::from(&flow), ..FlowSave::default() };
    let saved = now(&mut library.app, None, Command::FlowSave(save));

    assert!(matches!(imported, Response::Error(message) if message.contains("not among your personas")));
    assert!(matches!(saved, Response::Error(message) if message.contains("not among your personas")));
    assert_eq!(persona::list(&library.dir()).personas, Vec::new());
}

#[test]
fn a_check_says_when_a_file_it_would_replace_is_too_large_to_keep() {
    let mut library = Library::new();
    std::fs::create_dir_all(library.dir()).expect("the folder");
    std::fs::write(library.dir().join("reviewer.md"), "a".repeat(persona::MAX_BYTES + 1)).expect("write");

    let refused = library.refused(vec![text("reviewer.md", REVIEWER)], |import| {
        import.replace = true;
        import.check = true;
    });

    assert!(refused.contains("too large to keep a copy of"), "{refused}");
}

#[test]
fn a_flow_that_follows_a_persona_file_it_cannot_read_is_not_saved() {
    let mut library = Library::new();
    std::fs::create_dir_all(library.dir()).expect("the folder");
    std::fs::write(library.dir().join("reviewer.md"), "my notes").expect("write");
    let save = FlowSave { flow: FlowSpec::from(&following("reviewer")), ..FlowSave::default() };

    let saved = now(&mut library.app, None, Command::FlowSave(save));

    assert!(matches!(&saved, Response::Error(message) if message.contains("cannot be read")), "{saved:?}");
}

mod saving {
    use super::*;
    use crate::control::persona::{PersonaSave, PersonaSaved, PersonaShow, PersonaShown};
    use crate::flows::persona::draft::{self, Draft};

    fn tester(body: &str) -> String {
        let draft =
            Draft { name: "Careful Tester".into(), emoji: "🧪".into(), body: body.into(), ..Draft::default() };
        draft::compose(&draft, None)
    }

    fn save(library: &mut Library, id: &str, text: String, replace: bool) -> Response {
        now(&mut library.app, None, Command::PersonaSave(PersonaSave { id: id.into(), text, replace }))
    }

    fn saved(response: Response) -> PersonaSaved {
        match response {
            Response::Ok(value) => serde_json::from_value(value).expect("saved"),
            Response::Error(message) => panic!("not saved: {message}"),
        }
    }

    fn refused(response: Response) -> String {
        match response {
            Response::Error(message) => message,
            Response::Ok(value) => panic!("saved: {value}"),
        }
    }

    #[test]
    fn a_new_persona_takes_its_id_from_its_name_and_has_no_source() {
        let mut library = Library::new();

        let saved = saved(save(&mut library, "", tester("Test first."), false));

        assert_eq!((saved.persona.id.as_str(), saved.persona.source.as_deref()), ("careful-tester", None));
        let file = std::fs::read_to_string(library.dir().join("careful-tester.md")).expect("the file");
        assert_eq!(draft::body(&file), "Test first.\n");
        assert_eq!(library.listed().personas[0].emoji, "🧪");
    }

    #[test]
    fn an_id_or_a_name_you_have_is_refused_unless_it_replaces_that_id() {
        let mut library = Library::new();
        library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});

        let same_id = refused(save(&mut library, "reviewer", tester("x"), false));
        let renamed = REVIEWER.replace("You review code.", "You review code twice.");
        let same_name = refused(save(&mut library, "other", renamed.clone(), false));
        let replaced = saved(save(&mut library, "reviewer", renamed, true));

        assert!(same_id.contains("you already have the persona `reviewer`"), "{same_id}");
        assert!(same_name.contains("is also named `Code Reviewer`"), "{same_name}");
        assert!(replaced.persona.replaced);
        let file = std::fs::read_to_string(library.dir().join("reviewer.md")).expect("the file");
        assert!(file.contains("You review code twice."), "{file}");
    }

    #[rstest::rstest]
    #[case::not_an_id("Not An Id", tester("x"), "is not a persona id")]
    #[case::no_name("", "---\ndescription: x\n---\nbody\n".to_string(), "gives no `name`")]
    fn what_cannot_be_a_persona_is_refused(#[case] id: &str, #[case] text: String, #[case] reason: &str) {
        let mut library = Library::new();

        let message = refused(save(&mut library, id, text, false));

        assert!(message.contains(reason), "{message}");
        assert!(!library.dir().exists());
    }

    #[test]
    fn show_gives_the_whole_text_and_refuses_one_you_lack() {
        let mut library = Library::new();
        library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});

        let shown: PersonaShown = serde_json::from_value(value(
            &mut library.app,
            None,
            Command::PersonaShow(PersonaShow { id: "reviewer".into() }),
        ))
        .expect("shown");
        let missing = now(&mut library.app, None, Command::PersonaShow(PersonaShow { id: "gone".into() }));

        assert!(shown.text.starts_with("---\nname: Code Reviewer\n"), "{}", shown.text);
        assert_eq!(shown.persona.path, Some(library.dir().join("reviewer.md")));
        assert!(matches!(missing, Response::Error(_)), "{missing:?}");
    }

    #[test]
    fn flow_list_names_the_persona_of_each_role() {
        let mut library = Library::new();
        library.imported(vec![text("reviewer.md", REVIEWER)], |_| {});
        let save = FlowSave { flow: FlowSpec::from(&following("reviewer")), ..FlowSave::default() };
        assert!(matches!(now(&mut library.app, None, Command::FlowSave(save)), Response::Ok(_)));

        let flows: Flows =
            serde_json::from_value(value(&mut library.app, None, Command::FlowList(FlowList::default())))
                .expect("flows");

        let flow = flows.flows.iter().find(|flow| flow.name == "Reviewed").expect("the flow");
        let reviewer = flow.roles.iter().find(|role| role.role == "reviewer").expect("the reviewer");
        assert_eq!((reviewer.persona_name.as_str(), reviewer.persona_emoji.as_str()), ("Code Reviewer", "👁️"));
    }
}

mod a_catalogue {
    use super::*;

    #[test]
    fn answers_one_problem_per_persona_and_saves_nothing() {
        let mut library = Library::new();
        library.imported(vec![text("mine.md", REVIEWER)], |_| {});
        let tester = "---\nname: Tester\n---\nTest.\n";

        let answer = library.imported(
            vec![text("reviewer.md", REVIEWER), text("tester.md", tester), text("mine.md", REVIEWER)],
            |import| import.catalogue = true,
        );

        assert!(!answer.saved);
        let rows: Vec<(&str, Option<&str>, Option<&str>, bool)> = answer
            .personas
            .iter()
            .map(|p| (p.id.as_str(), p.file.as_deref(), p.problem.as_deref(), p.replaced))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "reviewer",
                    Some("reviewer.md"),
                    Some("another persona in the source is also named `Code Reviewer`"),
                    false
                ),
                ("tester", Some("tester.md"), None, false),
                ("mine", Some("mine.md"), Some("another persona in the source is also named `Code Reviewer`"), true),
            ]
        );
        assert!(!library.dir().join("tester.md").exists());
    }

    #[test]
    fn names_a_persona_whose_name_another_of_yours_has() {
        let mut library = Library::new();
        library.imported(vec![text("mine.md", REVIEWER)], |_| {});

        let answer = library.imported(vec![text("reviewer.md", REVIEWER)], |import| import.catalogue = true);

        assert_eq!(answer.personas[0].problem.as_deref(), Some("your persona `mine` is also named `Code Reviewer`"));
        assert_eq!(answer.check, crate::control::persona::CHECK);
    }
}
