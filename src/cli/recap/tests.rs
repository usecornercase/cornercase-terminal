use std::collections::BTreeMap;

use super::*;
use crate::recap::{Named, Place, Tokens};

fn info() -> RecapInfo {
    let mut places = BTreeMap::new();
    places.insert(
        "c1".to_string(),
        Place {
            kind: "conversation".into(),
            project: Some(Named { label: "api".into(), path: "/p".into() }),
            workspace: Some(Named { label: "rate-limit".into(), path: "/p/w".into() }),
            tab: Some("claude".into()),
            ..Place::default()
        },
    );
    places.insert("p1".to_string(), Place { kind: "pull".into(), number: Some(212), ..Place::default() });
    RecapInfo {
        range: "yesterday".into(),
        label: Some("yesterday · sat 31 oct".into()),
        title: Some("Claude Code".into()),
        conversations: 1,
        tokens: Some(Tokens { input: 1000, output: 500, cache_read: 0, cache_write: 0 }),
        age: Some(7200),
        text: Some("## On you\n- [c1] asked \"Redis?\" ([p1])".into()),
        places,
        ..RecapInfo::default()
    }
}

#[test]
fn the_text_has_a_header_and_names_places() {
    let text = text(&info());
    assert!(text.starts_with("yesterday · sat 31 oct · Claude Code · 1 conversation · "), "{text}");
    assert!(text.contains("2h ago"));
    assert!(text.contains("- api › rate-limit › claude asked \"Redis?\" (#212)"));
}

#[test]
fn a_dossier_is_printed_as_it_is() {
    let info = RecapInfo { dossier: Some("# Dossier".into()), ..RecapInfo::default() };
    assert_eq!(text(&info), "# Dossier");
}

#[test]
fn nothing_to_sum_up_says_why() {
    let info = RecapInfo { empty: Some("No agent activity in the last 30 days.".into()), ..RecapInfo::default() };
    assert_eq!(text(&info), "No agent activity in the last 30 days.\n");
}

#[test]
fn a_failure_follows_the_last_good_text() {
    let info = RecapInfo { error: Some("Not logged in".into()), ..info() };
    assert!(text(&info).ends_with("recap unavailable: Not logged in\n"));
}
