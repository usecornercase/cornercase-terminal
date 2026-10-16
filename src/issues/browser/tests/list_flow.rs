use super::*;

fn room(labels: &[&str]) -> usize {
    labels.iter().map(|label| label.chars().count() + 3).sum::<usize>() - 1
}

#[test]
fn a_narrow_list_drops_schedule_then_the_flow_so_start_stays() {
    let b = with_lists();
    let without_schedule = ["start", "start a flow", "refresh", "cancel"];
    assert_eq!(b.labels(room(&without_schedule)), without_schedule);
    assert_eq!(b.labels(room(&without_schedule) - 1), ["start", "refresh", "cancel"]);
}

#[test]
fn the_flow_button_starts_a_flow_for_the_selected_issue() {
    let mut b = with_lists();
    press(&mut b, KeyCode::Down);
    let pos = button(&b, "start a flow");
    assert!(matches!(click(&mut b, pos), Action::StartFlow(i, None) if i.key == "#1"));
}

#[test]
fn a_story_starts_its_flow_in_the_only_place() {
    let mut b = with_lists();
    let pos = button(&b, "start a flow");
    let started = click(&mut b, pos);
    assert!(
        matches!(&started, Action::StartFlow(i, Some(place)) if i.key == "sc-2" && *place == shop()),
        "{started:?}"
    );
}

#[test]
fn with_several_places_a_story_is_read_and_asks_where_first() {
    let mut b = Browser { places: vec![shop(), notes()], ..with_lists() };
    let pos = button(&b, "start a flow");
    let first = click(&mut b, pos);
    assert!(matches!(&first, Action::Read(i) if i.key == "sc-2"), "{first:?}");
    type_text(&mut b, "notes");
    let started = press(&mut b, KeyCode::Enter);
    assert!(
        matches!(&started, Action::StartFlow(i, Some(place)) if i.key == "sc-2" && *place == notes()),
        "{started:?}"
    );
}
