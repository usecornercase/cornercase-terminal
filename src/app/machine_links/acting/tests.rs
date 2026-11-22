use rstest::rstest;

use super::*;
use crate::control::PaneInfo;

const LINK: u64 = 1;
const SHELL: u64 = 11;
const AGENT: u64 = 13;

fn workspace() -> WorkspaceInfo {
    let tab = |id: u64, name: &str, panes: &[u64]| TabInfo {
        id,
        name: name.into(),
        panes: panes.iter().map(|&id| PaneInfo { id, ..PaneInfo::default() }).collect(),
        ..TabInfo::default()
    };
    WorkspaceInfo {
        id: 10,
        tabs: vec![tab(SHELL, "shell", &[12]), tab(AGENT, "claude", &[1, 2])],
        ..Default::default()
    }
}

fn acting(acts: &[Act]) -> Acting {
    let aim = Aim { link: LINK, era: 1, workspace: 10 };
    Acting { busy: acts.iter().map(|act| (aim, act.clone())).collect() }
}

mod a_tab_s_wait {
    use super::*;

    #[rstest]
    #[case::a_pane_of_a_closing_tab(&[Act::CloseTab(AGENT)], Scope::Pane(2), Some("claude"))]
    #[case::a_pane_of_another_tab(&[Act::CloseTab(AGENT)], Scope::Pane(12), None)]
    #[case::a_pane_of_a_tab_being_renamed(&[Act::RenameTab { tab: AGENT, name: "logs".into() }], Scope::Pane(1), None)]
    #[case::a_tab_being_renamed(&[Act::RenameTab { tab: AGENT, name: "logs".into() }], Scope::Tab(AGENT), Some("claude"))]
    #[case::a_closing_tab(&[Act::CloseTab(SHELL)], Scope::Tab(SHELL), Some("shell"))]
    #[case::the_workspace(&[Act::CloseTab(AGENT)], Scope::Workspace, None)]
    fn names_the_tab_an_act_on_this_scope_would_wait_for(
        #[case] acts: &[Act],
        #[case] scope: Scope,
        #[case] waits: Option<&str>,
    ) {
        let workspace = workspace();

        assert_eq!(waits_on(&acting(acts), LINK, &workspace, scope), waits);
    }

    #[test]
    fn is_kept_per_machine() {
        let workspace = workspace();

        assert_eq!(waits_on(&acting(&[Act::CloseTab(AGENT)]), LINK + 1, &workspace, Scope::Pane(2)), None);
    }
}
