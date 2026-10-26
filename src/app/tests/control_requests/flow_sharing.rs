use super::*;
use crate::control::{Exported, FlowExport, FlowImport, Imported};
use crate::flows::share::{self, MAX_BYTES};
use crate::flows::store;
use crate::flows::{Flow, ORCHESTRATOR, Role, Stage, builtin};
use crate::paths;

struct Share {
    app: App,
    _rx: Receiver<AppEvent>,
    repo: TempDir,
    config: TempDir,
}

fn role(name: &str, prompt: &str) -> Role {
    Role { name: name.into(), agent: "claude".into(), prompt: prompt.into(), ..Role::default() }
}

fn flow(name: &str) -> Flow {
    Flow {
        name: name.into(),
        orchestrator: role(ORCHESTRATOR, "SECRET-PROMPT-MARKER"),
        roles: vec![role("coder", "Write it.")],
        stages: vec![Stage { roles: vec!["coder".into()], back: None }],
        ..Flow::default()
    }
}

impl Share {
    fn new() -> Self {
        let (repo, config) = (TempDir::new(), TempDir::new());
        let config_path = write_config(config.path(), &Config::default());
        let (app, rx) = app_in(repo.path(), config_path);
        Self { app, _rx: rx, repo, config }
    }

    fn yours(&self) -> PathBuf {
        paths::flows(&self.config.path().join("config.json"))
    }

    fn project(&self) -> PathBuf {
        store::project_dir(self.repo.path())
    }

    fn put(dir: &Path, flow: &Flow) {
        store::save(dir, flow, None).expect("save the flow");
    }

    fn export(&mut self, name: &str) -> Response {
        now(&mut self.app, None, Command::FlowExport(FlowExport { name: name.into(), project: None }))
    }

    fn import(&mut self, text: &str, tweak: impl FnOnce(&mut FlowImport)) -> Response {
        let mut import = FlowImport { text: text.into(), ..FlowImport::default() };
        tweak(&mut import);
        now(&mut self.app, None, Command::FlowImport(import))
    }
}

fn exported(response: Response) -> Exported {
    match response {
        Response::Ok(value) => serde_json::from_value(value).expect("an export"),
        Response::Error(message) => panic!("the export failed: {message}"),
    }
}

fn imported(response: Response) -> Imported {
    match response {
        Response::Ok(value) => serde_json::from_value(value).expect("an import"),
        Response::Error(message) => panic!("the import failed: {message}"),
    }
}

fn text(name: &str) -> String {
    share::to_share(&flow(name)).expect("export")
}

mod exporting {
    use super::*;

    #[test]
    fn a_built_in_flow_yours_and_the_projects_come_out_as_text_that_reads_back() {
        let mut s = Share::new();
        Share::put(&s.yours(), &flow("Mine"));
        Share::put(&s.project(), &flow("Theirs"));
        for (name, origin) in [("Quick", "built-in"), ("mine", "user"), ("Theirs", "project")] {
            let out = exported(s.export(name));
            assert_eq!(out.origin, origin);
            let back = share::from_share(&out.text).expect("the text reads back");
            assert!(store::same_name(&back.name, name), "{name}");
        }
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_flows_there_are() {
        let message = error(Share::new().export("Nope"));
        assert!(message.contains("there is no flow named `Nope`") && message.contains("Quick"), "{message}");
    }

    #[test]
    fn a_flow_that_sets_variables_says_so_in_its_note() {
        let mut s = Share::new();
        let mut risky = flow("Risky");
        risky.roles[0].env.insert("AWS_PROFILE".into(), "work".into());
        Share::put(&s.yours(), &risky);
        let note = exported(s.export("Risky")).note.expect("a note");
        assert!(note.contains("AWS_PROFILE") && note.contains("no secret"), "{note}");
    }
}

mod importing {
    use super::*;

    #[test]
    fn into_yours_writes_a_file_in_the_flows_folder() {
        let mut s = Share::new();
        let done = imported(s.import(&text("Pair"), |_| {}));
        assert_eq!((done.name.as_str(), done.origin.as_str(), done.replaced), ("Pair", "user", false));
        assert_eq!(done.path, Some(s.yours().join("pair.toml")));
        assert_eq!(store::read(&s.yours().join("pair.toml")).expect("read").name, "Pair");
    }

    #[test]
    fn into_the_project_writes_to_its_flows_folder() {
        let mut s = Share::new();
        let done = imported(s.import(&text("Pair"), |i| i.place = "project".into()));
        assert_eq!(done.origin, "project");
        assert_eq!(done.path, Some(s.project().join("pair.toml")));
    }

    #[test]
    fn a_check_writes_nothing() {
        let mut s = Share::new();
        s.app.config.agent_commands.insert("grok".into(), "/nonexistent/grok".into());
        let mut flow = flow("Pair");
        flow.roles[0].agent = "grok".into();
        flow.roles[0].env.insert("TOKEN".into(), "demo".into());
        let done = imported(s.import(&share::to_share(&flow).expect("export"), |i| {
            i.check = true;
            i.name = Some("Renamed".into());
        }));
        assert_eq!(done.name, "Renamed");
        assert_eq!(done.roles.iter().map(|r| r.role.as_str()).collect::<Vec<_>>(), ["orchestrator", "coder"]);
        assert_eq!(done.roles[1].agent, "grok");
        assert!(done.check.expect("risks").contains("TOKEN=demo"));
        assert!(done.note.expect("missing agent").contains("not installed here"));
        assert_eq!(done.path, None);
        assert!(!s.yours().exists());
        assert!(error(s.import(&text("Quick"), |i| i.check = true)).contains("--replace"));
    }

    #[test]
    fn a_taken_name_is_refused_then_replaced_or_renamed() {
        let mut s = Share::new();
        Share::put(&s.yours(), &flow("Pair"));
        let refused = error(s.import(&text("pair"), |_| {}));
        assert!(refused.contains("--replace") && refused.contains("--name"), "{refused}");

        let mut changed = flow("Pair");
        changed.notes = "new".into();
        let done = imported(s.import(&share::to_share(&changed).expect("export"), |i| i.replace = true));
        assert!(done.replaced);
        assert_eq!(store::read(&s.yours().join("pair.toml")).expect("read").notes, "new");

        let renamed = imported(s.import(&text("Pair"), |i| i.name = Some("Pair mine".into())));
        assert_eq!(renamed.name, "Pair mine");
        assert!(s.yours().join("pair-mine.toml").exists());
    }

    #[test]
    fn a_built_in_name_needs_replace_and_the_project_cannot_take_it() {
        let mut s = Share::new();
        assert!(error(s.import(&text("Quick"), |_| {})).contains("built-in"));
        let done = imported(s.import(&text("Quick"), |i| i.replace = true));
        assert_eq!(done.origin, "user");
        let refused = error(s.import(&text("Quick"), |i| i.place = "project".into()));
        assert!(refused.contains("--name"), "{refused}");
    }

    #[test]
    fn a_project_flow_is_replaced_only_on_request() {
        let mut s = Share::new();
        Share::put(&s.project(), &flow("Theirs"));
        assert!(error(s.import(&text("Theirs"), |i| i.place = "project".into())).contains("--replace"));
        let done = imported(s.import(&text("Theirs"), |i| {
            i.place = "project".into();
            i.replace = true;
        }));
        assert!(done.replaced);
    }

    #[test]
    fn problems_bad_text_a_bad_place_and_too_much_text_are_refused() {
        let mut s = Share::new();
        let mut broken = flow("Broken");
        broken.roles[0].agent = "vim".into();
        let refused = error(s.import(&share::to_share(&broken).expect("export"), |_| {}));
        assert!(refused.contains("cannot run") && refused.contains("vim"), "{refused}");
        assert!(error(s.import("name = = 1", |_| {})).contains("line 1"));
        assert!(error(s.import(&text("Pair"), |i| i.place = "elsewhere".into())).contains("elsewhere"));
        let big = format!("{}#{}", text("Pair"), "x".repeat(MAX_BYTES));
        assert!(error(s.import(&big, |_| {})).contains("too large"));
        assert!(!s.yours().exists());
    }

    #[test]
    fn control_characters_in_an_imported_flow_never_reach_an_answer() {
        let mut s = Share::new();
        let mut bad = flow("Bad\x1b[2J");
        bad.roles[0].name = "co\x1b[1Ader".into();
        bad.stages[0].roles = vec!["co\x1b[1Ader".into()];
        bad.roles[0].agent = "vim\x1b[2J".into();
        bad.roles[0].env.insert("K\x1b[2J".into(), "v".into());
        let refused = error(s.import(&share::to_share(&bad).expect("export"), |_| {}));
        assert!(refused.contains("\\u{1b}[1Ader") && !refused.contains('\x1b'), "{refused:?}");
        s.app.config.agent_commands.insert("grok".into(), "/nonexistent/grok".into());
        let mut fine = flow("Fine");
        fine.roles[0].agent = "grok".into();
        let done = imported(s.import(&share::to_share(&fine).expect("export"), |_| {}));
        assert!(!done.note.expect("a note").contains('\x1b'));
        let taken = error(s.import(&share::to_share(&fine).expect("export"), |_| {}));
        assert!(!taken.contains('\x1b'), "{taken:?}");
    }

    #[test]
    fn agents_that_are_not_installed_here_are_noted() {
        let mut s = Share::new();
        s.app.config.agent_commands.insert("grok".into(), "/nonexistent/grok".into());
        let mut grok = flow("Grokky");
        grok.roles[0].agent = "grok".into();
        let note = imported(s.import(&share::to_share(&grok).expect("export"), |_| {})).note.expect("a note");
        assert!(note.contains("not installed here"), "{note}");
        assert!(builtin::installed(&s.app.config).len() < crate::agents::Known::ALL.len());
    }

    #[test]
    fn the_logged_fields_carry_sizes_and_never_the_text() {
        let command = Command::FlowImport(FlowImport { text: text("Pair"), ..FlowImport::default() });
        let fields = format!("{:?}", command.fields());
        assert!(fields.contains("text_bytes") && !fields.contains("SECRET-PROMPT-MARKER"), "{fields}");
    }
}
