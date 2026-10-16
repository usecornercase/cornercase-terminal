use super::*;

fn two_places() -> Browser {
    let mut b = Browser { places: vec![shop(), notes()], ..with_lists() };
    press(&mut b, KeyCode::Enter);
    b
}

#[test]
fn a_story_asks_where_first_with_the_current_project_preselected() {
    let mut b = two_places();
    let first = press(&mut b, KeyCode::Enter);
    let started = press(&mut b, KeyCode::Enter);
    assert_eq!(first, Action::None);
    assert!(matches!(&started, Action::Start(_, _, Some(place)) if *place == shop()), "{started:?}");
}

#[test]
fn another_place_can_be_picked() {
    let mut b = two_places();
    press(&mut b, KeyCode::Enter);
    type_text(&mut b, "notes");
    let started = press(&mut b, KeyCode::Enter);
    assert!(matches!(&started, Action::Start(_, _, Some(place)) if *place == notes()), "{started:?}");
}

#[test]
fn the_issue_shows_where_it_starts() {
    let mut b = two_places();
    b.place = Some(1);
    let start: String = b
        .detail_lines(ROOM)
        .into_iter()
        .find(|l| l.spans.first().is_some_and(|s| s.content.trim() == "start"))
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .expect("the start row");
    assert_eq!(start, "start   in a new tab in notes › default");
}

#[test]
fn where_is_offered_for_stories_only() {
    let mut b = two_places();
    let story = b.labels(ROOM.buttons);
    press(&mut b, KeyCode::Esc);
    press(&mut b, KeyCode::Down);
    press(&mut b, KeyCode::Enter);
    assert_eq!((story.contains(&"where…"), b.labels(ROOM.buttons).contains(&"where…")), (true, false));
}

#[test]
fn a_gitlab_issue_starts_in_its_repository_like_a_github_one() {
    let mut b = Browser { places: vec![shop(), notes()], ..browser() };
    b.connected(Source::Gitlab, Connection { from_env: true, account: None });
    b.tab_to(Tab::One(Source::Gitlab));
    b.loaded(Source::Gitlab, &Query::default(), Ok(vec![issue(Source::Gitlab, 5, "Checkout fails")]));
    press(&mut b, KeyCode::Enter);
    assert!(!b.labels(ROOM.buttons).contains(&"where…"), "{:?}", b.labels(ROOM.buttons));
    assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(i, _, None) if i.key == "#5"));
}

#[test]
fn a_flow_for_a_story_asks_where_first_then_starts_there() {
    let mut b = two_places();
    let pos = button(&b, "start a flow");
    assert_eq!(click(&mut b, pos), Action::None);
    type_text(&mut b, "notes");
    let started = press(&mut b, KeyCode::Enter);
    assert!(matches!(&started, Action::StartFlow(i, Some(place)) if i.key == "sc-2" && *place == notes()));
}

#[test]
fn a_flow_for_a_github_issue_starts_in_its_repository() {
    let mut b = two_places();
    press(&mut b, KeyCode::Esc);
    press(&mut b, KeyCode::Down);
    press(&mut b, KeyCode::Enter);
    let pos = button(&b, "start a flow");
    assert!(matches!(click(&mut b, pos), Action::StartFlow(i, None) if i.key == "#1"));
}

#[test]
fn a_github_issue_starts_in_its_repository() {
    let mut b = two_places();
    press(&mut b, KeyCode::Esc);
    press(&mut b, KeyCode::Down);
    press(&mut b, KeyCode::Enter);
    assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(i, _, None) if i.key == "#1"));
}
