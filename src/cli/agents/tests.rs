use super::*;
use crate::control::arrange::PaneRef;

fn pane(pane: u64, role: Option<&str>, resumes: bool) -> ClosedPane {
    ClosedPane {
        pane,
        tab: 5,
        agent: Some("claude".into()),
        role: role.map(str::to_string),
        status: Some("working".into()),
        resumes,
    }
}

fn closed(done: bool) -> AgentsClosed {
    AgentsClosed {
        project: 1,
        workspace: 7,
        tabs: vec![5],
        agents: vec![pane(31, Some("coder"), true), pane(32, None, false)],
        stops: vec![PaneRef { pane: 33, agent: None, project: 1, workspace: 7, tab: 5 }],
        run: None,
        closed: done,
        closed_agents: 2,
    }
}

#[test]
fn a_close_teaches_the_way_back() {
    assert_eq!(
        closed_text(&closed(true)),
        "closed 2 agents in workspace 7; cornercase agents reopen --workspace 7 reopens them\n"
    );
}

#[test]
fn a_check_lists_what_would_close_and_what_stops_with_it() {
    assert_eq!(
        closed_text(&closed(false)),
        "would close 2 agents in workspace 7:\n  pane 31  claude  coder  working  resumes\n  pane 32  claude  -  working  \
         starts over\n  also stops pane 33 in tab 5\n"
    );
}

#[test]
fn a_reopen_says_which_agents_resume() {
    let reopened = AgentsReopened {
        project: 1,
        workspace: 7,
        tabs: vec![8, 9],
        panes: vec![pane(41, Some("coder"), true), pane(42, None, false)],
        run: None,
    };

    assert_eq!(reopened_text(&reopened), "pane 41  claude  coder  resumes\npane 42  claude  -  starts over\n");
}
