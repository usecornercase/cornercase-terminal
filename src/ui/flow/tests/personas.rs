use super::*;

fn following(id: &str) -> Diagram {
    let mut flow = full_issue();
    flow.roles[0].persona = Some(id.into());
    Diagram::plan(&flow, MUTED, false)
}

#[test]
fn a_role_box_names_its_persona_after_its_agent() {
    let labels = vec![("reviewer".to_string(), "Code Reviewer".to_string())];
    let t = render(&following("reviewer").named(&labels), 140);
    assert!(find(&t, " · Code Reviewer").is_some());
}

#[test]
fn a_persona_without_a_label_shows_its_id() {
    let t = render(&following("reviewer"), 140);
    assert!(find(&t, " · reviewer").is_some());
}

#[test]
fn a_persona_that_does_not_fit_is_left_out() {
    let labels = vec![("reviewer".to_string(), "A Very Long Persona Name That Never Fits".to_string())];
    let t = render(&following("reviewer").named(&labels), 60);
    assert!(find(&t, "A Very Long").is_none());
}
