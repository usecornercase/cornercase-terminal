use serde_json::{Value, json};

use super::*;
use crate::control::config::SectionPreview;
use crate::settings::Page;

const SHOP: &str = "github.com/acme/web-shop";

fn settings() -> Settings {
    Settings::new(Config::default(), None, Vec::new(), Page::default())
}

fn nobody() -> Here {
    Here { home: None, projects: Vec::new(), connected: Vec::new() }
}

fn open(id: u64, folder: &str, remotes: &[&str]) -> OpenRepository {
    OpenRepository { id, folder: PathBuf::from(folder), remotes: remotes.iter().map(|r| (*r).to_string()).collect() }
}

fn shop(setup: &str) -> Repository {
    Repository { remotes: vec![SHOP.into()], setup: setup.into(), teardown: String::new() }
}

fn with(values: &Value) -> Shared {
    let Value::Object(values) = values.clone() else { panic!("an object") };
    Shared { settings: values.into_iter().collect(), ..Shared::default() }
}

fn section<'a>(preview: &'a SetupPreview, id: &str) -> &'a SectionPreview {
    preview.sections.iter().find(|section| section.id == id).expect("the section")
}

fn notes_of(preview: &SetupPreview, kind: NoteKind) -> Vec<String> {
    let all = preview.sections.iter().flat_map(|section| &section.notes).chain(&preview.notes);
    all.filter(|note| note.kind == kind).map(|note| note.text.clone()).collect()
}

fn note(kind: NoteKind, text: &str) -> SetupNote {
    SetupNote { kind, text: text.into() }
}

mod changes {
    use super::*;

    #[test]
    fn lists_only_what_differs_with_the_rows_labels() {
        let settings = settings();
        let label = settings.describe().into_iter().find(|s| s.key == "sidebar").expect("a row").label;

        let preview = preview(&with(&json!({"sidebar": "tree", "counts": true})), &settings, &nobody());

        let wanted = SetupChange { key: "sidebar".into(), label, from: "projects_on_top".into(), to: "tree".into() };
        assert_eq!(section(&preview, "interface").changes, [wanted]);
    }

    #[test]
    fn shows_a_setting_without_a_row_as_its_value() {
        let preview = preview(&with(&json!({"prompt": "Work on {url}"})), &settings(), &nobody());

        let wanted = SetupChange {
            key: "prompt".into(),
            label: "prompt".into(),
            from: "\"{url}\"".into(),
            to: "\"Work on {url}\"".into(),
        };
        assert_eq!(section(&preview, "agents").changes, [wanted]);
    }

    #[test]
    fn a_section_with_nothing_new_has_no_changes() {
        let shared = Shared { settings: settings_of(&Config::default()), ..Shared::default() };

        let preview = preview(&shared, &settings(), &nobody());

        let ids: Vec<&str> = preview.sections.iter().map(|section| section.id.as_str()).collect();
        assert_eq!(ids, ["agents", "issues", "worktrees", "interface", "repositories"]);
        assert!(preview.sections.iter().all(|section| section.changes.is_empty()), "{preview:?}");
    }

    #[test]
    fn a_repository_changes_the_setup_of_its_open_project() {
        let here = Here { projects: vec![open(1, "/work/web-shop", &[SHOP])], ..nobody() };
        let shared = Shared { repositories: vec![shop("npm ci")], ..Shared::default() };

        let preview = preview(&shared, &settings(), &here);

        let wanted = SetupChange {
            key: "repositories.setup".into(),
            label: "/work/web-shop".into(),
            from: "nothing".into(),
            to: "npm ci".into(),
        };
        assert_eq!(section(&preview, "repositories").changes, [wanted]);
    }

    #[test]
    fn a_setup_and_a_teardown_are_changes_of_their_own_in_full() {
        let here = Here { projects: vec![open(1, "/work/web-shop", &[SHOP])], ..nobody() };
        let repository = Repository { teardown: "./clean.sh".into(), ..shop("npm ci\ncp .env.example .env") };
        let shared = Shared { repositories: vec![repository], ..Shared::default() };

        let preview = preview(&shared, &settings(), &here);

        let changes: Vec<(&str, &str)> = section(&preview, "repositories")
            .changes
            .iter()
            .map(|change| (change.key.as_str(), change.to.as_str()))
            .collect();
        assert_eq!(
            changes,
            [("repositories.setup", "npm ci\ncp .env.example .env"), ("repositories.teardown", "./clean.sh")]
        );
        assert_eq!(
            notes_of(&preview, NoteKind::Risk),
            [
                "runs these setup commands in every new worktree of /work/web-shop",
                "runs these teardown commands before a worktree of /work/web-shop is removed",
            ]
        );
    }
}

mod notes {
    use super::*;

    #[test]
    fn name_a_missing_sound_file() {
        let preview = preview(&with(&json!({"sound_waiting": "/nonexistent/ding.wav"})), &settings(), &nobody());

        assert_eq!(
            section(&preview, "interface").notes,
            [note(NoteKind::Missing, "/nonexistent/ding.wav does not exist here")]
        );
    }

    #[test]
    fn name_a_program_not_on_the_path() {
        let shared = with(&json!({"gh": "no-such-gh-here", "worktrees_dir": "/nonexistent/worktrees"}));

        let preview = preview(&shared, &settings(), &nobody());

        assert_eq!(
            section(&preview, "worktrees").notes,
            [
                note(
                    NoteKind::Missing,
                    "/nonexistent/worktrees does not exist here yet: it will be made with the first worktree"
                ),
                note(NoteKind::Risk, "runs no-such-gh-here for GitHub"),
                note(NoteKind::Missing, "no-such-gh-here is not on the PATH here"),
            ]
        );
    }

    #[test]
    fn name_a_program_path_that_does_not_exist() {
        let shared = with(&json!({"agent_commands": {"claude": "/nonexistent/claude"}}));

        let preview = preview(&shared, &settings(), &nobody());

        assert_eq!(
            section(&preview, "agents").notes,
            [
                note(NoteKind::Risk, "runs /nonexistent/claude as the claude agent"),
                note(NoteKind::Missing, "/nonexistent/claude does not exist here"),
            ]
        );
    }

    #[test]
    fn mark_setup_commands_and_a_dangerous_mode_as_risks() {
        let mut shared = with(&json!({
            "agent_args": {"claude": ["--dangerously-skip-permissions"]},
            "accept_trust_prompts": true,
        }));
        shared.repositories = vec![shop("npm ci")];
        let here = Here { projects: vec![open(1, "/work/web-shop", &[SHOP])], ..nobody() };

        let preview = preview(&shared, &settings(), &here);

        assert_eq!(
            notes_of(&preview, NoteKind::Risk),
            [
                "starts claude in skip permissions (dangerous) mode",
                "answers agents' trust questions by itself",
                "runs these setup commands in every new worktree of /work/web-shop",
            ]
        );
    }

    #[test]
    fn leave_a_dangerous_mode_already_in_use_alone() {
        let args = json!({"claude": ["--dangerously-skip-permissions"], "codex": ["--model", "o3"]});
        let mut config = Config::default();
        config.agent_args.insert("claude".into(), vec!["--dangerously-skip-permissions".into()]);
        let settings = Settings::new(config, None, Vec::new(), Page::default());

        let preview = preview(&with(&json!({"agent_args": args})), &settings, &nobody());

        assert_eq!(notes_of(&preview, NoteKind::Risk), Vec::<String>::new());
    }

    #[test]
    fn name_trackers_to_connect_and_unknown_keys() {
        let shared = Shared {
            connect: vec!["jira".into(), "linear".into()],
            unknown: vec!["interface.colours".into()],
            ..Shared::default()
        };
        let here = Here { connected: vec!["linear".into()], ..nobody() };

        let preview = preview(&shared, &settings(), &here);

        assert_eq!(
            preview.notes,
            [
                note(
                    NoteKind::Unknown,
                    "this cornercase does not know interface.colours; update it to import that setting"
                ),
                note(NoteKind::Connect, "connect jira in Settings › Issues"),
            ]
        );
    }

    #[test]
    fn name_a_possible_secret() {
        let shared = Shared { repositories: vec![shop("export NPM_TOKEN=abc")], ..Shared::default() };

        let preview = preview(&shared, &settings(), &nobody());

        assert_eq!(
            notes_of(&preview, NoteKind::Secret),
            ["setup of github.com/acme/web-shop sets NPM_TOKEN: check no secret goes with it"]
        );
    }

    #[test]
    fn name_a_repository_no_open_project_has() {
        let shared = Shared { repositories: vec![shop("npm ci")], ..Shared::default() };

        let preview = preview(&shared, &settings(), &nobody());

        assert_eq!(
            section(&preview, "repositories").notes,
            [note(
                NoteKind::Repository,
                "github.com/acme/web-shop: no open project has this remote; open it and import again"
            )]
        );
        assert_eq!(section(&preview, "repositories").changes, []);
    }
}

mod matching {
    use super::*;

    fn found(remotes: &[&str], projects: &[&[&str]]) -> Vec<u64> {
        let repository =
            Repository { remotes: remotes.iter().map(|r| (*r).to_string()).collect(), ..Repository::default() };
        let projects = (1..).zip(projects).map(|(id, remotes)| open(id, &format!("/{id}"), remotes)).collect();
        matches(&repository, &Here { projects, ..nobody() }).iter().map(|project| project.id).collect()
    }

    #[test]
    fn a_fork_matches_by_its_upstream() {
        assert_eq!(found(&["github.com/alice/web-shop", SHOP], &[&[SHOP], &["github.com/bob/api"]]), [1]);
    }

    #[test]
    fn two_clones_of_one_remote_both_match() {
        assert_eq!(found(&[SHOP], &[&[SHOP], &[SHOP]]), [1, 2]);
    }

    #[test]
    fn hosts_compare_without_case() {
        assert_eq!(found(&["GitHub.com/acme/web-shop"], &[&[SHOP], &["github.com/ACME/web-shop"]]), [1]);
    }
}
