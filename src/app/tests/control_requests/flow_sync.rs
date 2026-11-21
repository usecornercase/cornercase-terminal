use super::*;
use crate::control::FlowImport;
use crate::flows::store;
use crate::flows::sync::NEWER_HERE;
use crate::flows::{Flow, ORCHESTRATOR, Role, Stage, share};
use crate::paths;

struct Here {
    app: App,
    _rx: Receiver<AppEvent>,
    config: TempDir,
}

fn duo(changed: &str, notes: &str) -> Flow {
    let role =
        |name: &str| Role { name: name.into(), agent: "claude".into(), prompt: "Do it.".into(), ..Role::default() };
    Flow {
        name: "Duo".into(),
        changed: Some(changed.into()),
        notes: notes.into(),
        orchestrator: role(ORCHESTRATOR),
        roles: vec![role("coder")],
        stages: vec![Stage { roles: vec!["coder".into()], back: None }],
        ..Flow::default()
    }
}

impl Here {
    fn with_a_copy_saved_at(changed: &str) -> Self {
        let config = TempDir::new();
        let (app, rx) = new_app(write_config(config.path(), &Config::default()));
        let here = Self { app, _rx: rx, config };
        store::save(&here.yours(), &duo(changed, "here"), None).expect("the copy here");
        here
    }

    fn yours(&self) -> PathBuf {
        paths::flows(&self.config.path().join("config.json"))
    }

    fn synced_from_laptop(&mut self, flow: &Flow) -> Response {
        let text = share::to_share(flow).expect("share");
        let from = Some("laptop".into());
        let import =
            FlowImport { text, place: "yours".into(), replace: true, synced_from: from, ..FlowImport::default() };
        now(&mut self.app, None, Command::FlowImport(import))
    }

    fn copy_here(&self) -> Flow {
        store::read(&self.yours().join("duo.toml")).expect("the copy here")
    }
}

mod an_import_from_a_machine {
    use super::*;

    #[test]
    fn older_than_the_copy_here_is_refused_and_the_copy_stays() {
        let mut here = Here::with_a_copy_saved_at("2026-11-20T12:00:00.000Z");

        let answer = here.synced_from_laptop(&duo("2026-11-20T10:00:00.000Z", "theirs"));

        let Response::Error(why) = answer else { panic!("refused: {answer:?}") };
        assert!(why.contains(NEWER_HERE), "{why}");
        assert_eq!(here.copy_here().notes, "here");
    }

    #[test]
    fn newer_replaces_the_copy_here_keeps_its_time_and_says_where_it_came_from() {
        let mut here = Here::with_a_copy_saved_at("2026-11-20T12:00:00.000Z");

        let answer = here.synced_from_laptop(&duo("2026-11-20T14:00:00.000Z", "theirs"));

        assert!(matches!(answer, Response::Ok(_)), "{answer:?}");
        let copy = here.copy_here();
        assert_eq!((copy.notes.as_str(), copy.changed.as_deref()), ("theirs", Some("2026-11-20T14:00:00.000Z")));
        assert_eq!(toast(&here.app), Some("Duo was replaced by laptop's copy: it synced its flows here"));
    }
}

mod what_a_sync_sends {
    use super::*;

    fn names(failed: &[crate::control::FlowSyncFailure]) -> Vec<&str> {
        failed.iter().map(|failed| failed.name.as_str()).collect()
    }

    #[test]
    fn leaves_out_a_file_of_yours_it_cannot_read_says_why_and_still_sends_the_others() {
        let here = Here::with_a_copy_saved_at("2026-11-20T12:00:00.000Z");
        std::fs::write(here.yours().join("newer.toml"), "version = 99\nname = \"Newer\"\n").expect("a broken file");

        let outgoing = here.app.flows_to_send(None).expect("what is sent");

        let sent: Vec<&str> = outgoing.flows.iter().map(|held| held.name.as_str()).collect();
        assert_eq!((sent, names(&outgoing.left_out)), (vec!["Duo"], vec!["newer.toml"]));
        assert!(outgoing.left_out[0].error.contains("newer.toml"), "{:?}", outgoing.left_out);
    }

    #[test]
    fn leaves_out_both_files_of_yours_that_share_a_name_and_names_them() {
        let here = Here::with_a_copy_saved_at("2026-11-20T12:00:00.000Z");
        std::fs::write(
            here.yours().join("copy.toml"),
            share::to_share(&duo("2026-11-20T13:00:00.000Z", "copy")).expect("share"),
        )
        .expect("a second file named Duo");

        for name in [None, Some("Duo")] {
            let outgoing = here.app.flows_to_send(name).expect("what is sent");

            assert_eq!((outgoing.flows.len(), names(&outgoing.left_out)), (0, vec!["Duo"]), "{name:?}");
            let why = &outgoing.left_out[0].error;
            assert!(why.contains("copy.toml") && why.contains("duo.toml"), "{why}");
        }
    }

    #[test]
    fn takes_a_folder_of_your_flows_not_made_yet_as_no_flows_at_all() {
        let config = TempDir::new();
        let (app, _rx) = new_app(write_config(config.path(), &Config::default()));

        let outgoing = app.flows_to_send(None).expect("what is sent");

        assert_eq!((outgoing.flows.len(), outgoing.left_out.len()), (0, 0));
    }
}
