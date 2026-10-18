use super::*;

fn removed(changed: bool, deleted: Option<&str>) -> FlowRemoved {
    FlowRemoved {
        name: "Quick".into(),
        origin: Some("user".into()),
        deleted: deleted.map(Into::into),
        hidden: false,
        changed,
    }
}

#[test]
fn a_deleted_flow_names_its_place_and_its_file() {
    assert_eq!(deleted_line(&removed(true, Some("/c/flows/mine.toml"))), "deleted Quick (yours): /c/flows/mine.toml\n");
    let project = FlowRemoved { origin: Some("project".into()), ..removed(true, None) };
    assert_eq!(deleted_line(&project), "deleted Quick (this project's)\n");
}

#[test]
fn hiding_says_how_to_bring_the_flow_back_and_what_else_went() {
    assert_eq!(
        hidden_line(&removed(true, None)),
        "deleted the built-in flow Quick; `cornercase flow restore Quick` brings it back\n"
    );
    assert!(
        hidden_line(&removed(true, Some("/c/flows/quick.toml"))).contains(", and your version /c/flows/quick.toml;")
    );
    assert_eq!(hidden_line(&removed(false, None)), "the built-in flow Quick was already deleted\n");
}

#[test]
fn restoring_says_what_was_deleted_or_that_there_was_nothing() {
    assert_eq!(
        restored_line(&removed(true, Some("/c/flows/quick.toml"))),
        "restored the built-in flow Quick; your version /c/flows/quick.toml is deleted\n"
    );
    assert_eq!(restored_line(&removed(true, None)), "restored the built-in flow Quick\n");
    assert!(restored_line(&removed(false, None)).contains("nothing to restore"));
}

#[test]
fn the_list_ends_with_the_deleted_built_ins_when_there_are_some() {
    let hidden = FlowInfo { name: "Quick".into(), origin: "built-in".into(), ..FlowInfo::default() };
    let flows = Flows { hidden: vec![hidden], ..Flows::default() };
    assert_eq!(listing(&flows), "deleted built-in flows: Quick (`cornercase flow restore NAME` brings one back)\n");
    assert_eq!(listing(&Flows::default()), "");
}
