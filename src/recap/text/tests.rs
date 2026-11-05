use super::*;
use crate::recap::Named;

fn places() -> BTreeMap<String, Place> {
    let mut places = BTreeMap::new();
    places.insert(
        "c1".to_string(),
        Place {
            kind: "conversation".into(),
            project: Some(Named { label: "api".into(), path: "/a".into() }),
            workspace: Some(Named { label: "rate-limit".into(), path: "/a/w".into() }),
            tab: Some("claude".into()),
            ..Place::default()
        },
    );
    places.insert("p1".to_string(), Place { kind: "pull".into(), number: Some(212), ..Place::default() });
    places
}

#[test]
fn plain_text_names_places_and_pull_requests() {
    let text = "- [c1] asked \"Redis or memory?\" ([p1])\n- [c9] unknown";
    assert_eq!(plain(text, &places()), "- api › rate-limit › claude asked \"Redis or memory?\" (#212)\n- [c9] unknown");
}

#[test]
fn blocks_hold_headings_bullets_links_bold_and_quotes() {
    let blocks = parse("## On you\n\n- **[c1]**: asked \"Redis?\" now\n", &places());
    assert_eq!(blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), [Kind::Heading, Kind::Blank, Kind::Bullet]);
    let bullet = &blocks[2].segments;
    assert_eq!(bullet[0].link.as_deref(), Some("c1"));
    assert!(bullet[0].bold);
    assert_eq!(bullet[0].text, "api › rate-limit › claude");
    let quoted: Vec<&str> = bullet.iter().filter(|s| s.quote).map(|s| s.text.as_str()).collect();
    assert_eq!(quoted, ["\"Redis?\""]);
}

#[test]
fn an_unknown_id_stays_as_typed_and_is_no_link() {
    let blocks = parse("see [c7] and [not an id]", &places());
    assert!(blocks[0].segments.iter().all(|s| s.link.is_none()));
    let text: String = blocks[0].segments.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(text, "see [c7] and [not an id]");
}
