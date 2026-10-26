use std::path::{Path, PathBuf};

use super::*;
use crate::plugins::registry::{Entry, Source};

fn evil() -> Plugin {
    let entry = Entry {
        id: "notes\u{202e}".into(),
        source: Source::Link { path: PathBuf::from("/plugins/evil\u{202e}lmth.sh\u{e0041}") },
        enabled: false,
    };
    Plugin { entry, manifest: Err("cannot read it".into()) }
}

#[test]
fn the_list_shows_invisible_characters_escaped() {
    let text = list_text(&[evil()]);
    assert!(text.contains("notes\\u{202e}  -  off") && text.contains("evil\\u{202e}lmth.sh\\u{e0041}"), "{text}");
}

#[test]
fn the_json_list_escapes_invisible_characters_and_keeps_their_values() {
    let json = list_json(&[evil()], &Places::new(Path::new("/config"), Path::new("/state")));
    assert!(!json.chars().any(|c| c != '\n' && crate::text::hidden(c)), "{json}");
    assert!(json.contains("evil\\u202elmth.sh\\udb40\\udc41"), "{json}");
    let read: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(read["plugins"][0]["id"], "notes\u{202e}");
}

fn info(actions: Vec<crate::control::plugins::ActionInfo>) -> ActionsInfo {
    ActionsInfo { revision: 2, actions }
}

fn listed(plugins: Vec<PluginInfo>) -> PluginsInfo {
    PluginsInfo { revision: Some(3), problem: None, plugins }
}

fn on_plugin() -> PluginInfo {
    PluginInfo {
        id: "acme.notes".into(),
        enabled: true,
        state: "on".into(),
        label: "Notes 1.0".into(),
        note: "service sync runs".into(),
        services: vec![
            ServiceInfo { id: "sync".into(), state: "running".into(), pid: Some(4242), ..ServiceInfo::default() },
            ServiceInfo { id: "watch".into(), state: "stopped".into(), restart_in: Some(3), ..ServiceInfo::default() },
            ServiceInfo { id: "late".into(), state: "starting".into(), ..ServiceInfo::default() },
            ServiceInfo { id: "odd".into(), state: "paused".into(), ..ServiceInfo::default() },
        ],
        button: Some(ButtonInfo { title: "Notes".into(), icon: "N".into(), actions: vec!["open".into()] }),
        ..PluginInfo::default()
    }
}

#[test]
fn the_status_names_each_plugin_its_note_and_what_each_service_is_doing() {
    let broken = PluginInfo {
        id: "acme.broken".into(),
        state: "cannot-run".into(),
        label: "acme.broken".into(),
        note: "cannot read it".into(),
        ..PluginInfo::default()
    };
    let text = status_text(&listed(vec![on_plugin(), broken]));
    assert_eq!(
        text,
        "\
revision 3
acme.notes  on  Notes 1.0
  service sync runs
  sync: running, pid 4242
  watch: stopped, restarts in 3s
  late: starting
  odd: paused
  button Notes (N): open
acme.broken  cannot-run  acme.broken
  cannot read it
"
    );
}

#[test]
fn the_status_says_when_there_are_no_plugins_and_when_the_registry_cannot_be_read() {
    assert_eq!(status_text(&PluginsInfo::default()), "no plugins\n");
    let unread = PluginsInfo { revision: Some(4), problem: Some("cannot read plugins.json".into()), plugins: vec![] };
    assert_eq!(status_text(&unread), "revision 4\ncannot read plugins.json\n");
}

#[test]
fn the_status_json_escapes_invisible_characters_and_keeps_their_values() {
    let plugin = PluginInfo {
        id: "notes\u{202e}".into(),
        source: serde_json::json!("/plugins/evil\u{202e}lmth.sh"),
        ..PluginInfo::default()
    };
    let json = shown_json(&listed(vec![plugin]));
    assert!(!json.chars().any(|c| c != '\n' && crate::text::hidden(c)), "{json}");
    let read: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(read["plugins"][0]["id"], "notes\u{202e}");
    assert_eq!(read["plugins"][0]["source"], "/plugins/evil\u{202e}lmth.sh");
}

#[test]
fn the_status_escapes_invisible_characters() {
    let plugin = PluginInfo {
        id: "notes\u{202e}".into(),
        state: "off".into(),
        label: "Notes".into(),
        note: "a\nb".into(),
        ..PluginInfo::default()
    };
    let text = status_text(&listed(vec![plugin]));
    assert!(!text.chars().any(|c| c != '\n' && c.is_control()), "{text}");
    assert!(text.contains("notes\\u{202e}  off  Notes") && text.contains("  a b"), "{text}");
}

#[test]
fn the_actions_are_a_tab_separated_table_under_a_header() {
    let action = crate::control::plugins::ActionInfo {
        plugin: "acme.notes".into(),
        name: "Notes".into(),
        action: "note".into(),
        title: "take\ta\nnote".into(),
        contexts: vec!["pane".into(), "tab".into()],
        ..Default::default()
    };
    assert_eq!(
        actions_text(&info(vec![action])),
        "plugin\taction\tcontexts\ttitle\nacme.notes\tnote\tpane,tab\ttake a note\n"
    );
    assert_eq!(actions_text(&info(Vec::new())), "plugin\taction\tcontexts\ttitle\n");
}

#[test]
fn a_run_says_how_it_ended() {
    let ran = |error: Option<&str>| ActionRan {
        plugin: "acme.notes".into(),
        action: "note".into(),
        error: error.map(str::to_string),
        ..ActionRan::default()
    };
    assert_eq!(ran_text(&ran(None), Some(Item::Pane(5))), "ran acme.notes note on pane 5: exit status: 0\n");
    assert_eq!(
        ran_text(&ran(Some("exit status: 3")), Some(Item::Tab(2))),
        "ran acme.notes note on tab 2: exit status: 3; `cornercase plugin logs acme.notes` says more\n"
    );
    assert_eq!(
        ran_text(&ran(Some("the popup was closed")), None).lines().next(),
        Some("ran acme.notes note: the popup was closed; `cornercase plugin logs acme.notes` says more")
    );
}

mod previewing {
    use super::*;
    use crate::cli::args::{Command as Cli, PluginAction};
    use crate::cli::tests::parse;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn parses(args: &[&str]) -> bool {
        let mut line = vec!["plugin"];
        line.extend(args);
        parse(&line).is_ok()
    }

    fn install(args: &[&str]) -> InstallArgs {
        let mut line = vec!["plugin", "install", "acme/notes"];
        line.extend(args);
        match parse(&line).expect("parse").command {
            Some(Cli::Plugin(plugin)) => match plugin.action {
                PluginAction::Install(args) => args,
                _ => panic!("not install"),
            },
            _ => panic!("not plugin"),
        }
    }

    #[test]
    fn an_install_or_update_previews_without_asking_and_takes_the_commit_it_showed() {
        let args = install(&["--preview", "--json"]);
        assert!(args.preview && args.json && !args.yes && args.commit.is_none());
        let args = install(&["--commit", &SHA.to_ascii_uppercase(), "--yes"]);
        assert_eq!(args.commit.as_deref(), Some(SHA));
        for bad in [
            vec!["install", "a/b", "--preview", "--yes"],
            vec!["install", "a/b", "--json"],
            vec!["install", "a/b", "--commit", "0123abc"],
            vec!["install", "a/b", "--commit", SHA, "--preview"],
            vec!["update", "--preview", "--json"],
            vec!["update", "--commit", SHA],
        ] {
            assert!(!parses(&bad), "{bad:?}");
        }
        assert!(parses(&["update", "--preview"]) && parses(&["update", "acme.notes", "--preview", "--json"]));
    }

    #[test]
    fn another_commit_than_the_one_named_is_refused() {
        let (a, b) = (SHA, "fedcba9876543210fedcba9876543210fedcba98");
        assert!(same_commit(a, None, "acme/notes", "nothing was installed").is_ok());
        assert!(same_commit(a, Some(&a.to_ascii_uppercase()), "acme/notes", "nothing was installed").is_ok());
        let error = same_commit(a, Some(b), "acme/notes", "nothing was installed").expect_err("refused").to_string();
        assert!(
            error.contains(&a[..12]) && error.contains(&b[..12]) && error.contains("nothing was installed"),
            "{error}"
        );
    }
}
