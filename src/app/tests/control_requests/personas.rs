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
    _repo: TempDir,
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
        let repo = TempDir::new();
        let (app, rx) = app_in(repo.path(), write_config(config.path(), &Config::default()));
        Self { app, _rx: rx, _repo: repo, config }
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
    assert_eq!(persona::ids(&library.dir()), Vec::<String>::new());
}
