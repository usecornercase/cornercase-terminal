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

#[test]
fn the_actions_are_a_tab_separated_table_under_a_header() {
    let action = crate::control::plugins::ActionInfo {
        plugin: "acme.notes".into(),
        name: "Notes".into(),
        action: "note".into(),
        title: "take\ta\nnote".into(),
        contexts: vec!["pane".into(), "tab".into()],
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
    assert_eq!(ran_text(&ran(None), Item::Pane(5)), "ran acme.notes note on pane 5: exit status: 0\n");
    assert_eq!(
        ran_text(&ran(Some("exit status: 3")), Item::Tab(2)),
        "ran acme.notes note on tab 2: exit status: 3; `cornercase plugin logs acme.notes` says more\n"
    );
}
