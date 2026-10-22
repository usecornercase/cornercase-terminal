use super::*;

#[test]
fn a_workspace_with_closed_agents_says_how_many() {
    let at = |closed_agents| {
        let workspace = WorkspaceInfo { name: "fix".into(), closed_agents, ..WorkspaceInfo::default() };
        workspace_parts(&workspace, None).join("  ")
    };

    assert!(at(Some(4)).ends_with("  4 agents closed"), "{}", at(Some(4)));
    assert!(at(Some(1)).ends_with("  1 agent closed"), "{}", at(Some(1)));
    assert!(!at(None).contains("closed"));
}
