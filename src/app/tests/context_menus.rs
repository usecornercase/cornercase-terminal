use super::close_agent::agent;
use super::*;
use crate::app::menus::MenuAction;

fn labels(app: &App, tab: usize) -> Vec<String> {
    let id = app.projects[0].workspaces[0].tabs[tab].id;
    let actions = app.target_menu(Target::Tab(app.projects[0].id, app.projects[0].workspaces[0].id, id));
    app.menu_labels(&actions).into_iter().filter(|label| label.starts_with("view context")).collect()
}

#[test]
fn a_tab_with_one_agent_lists_view_context_once() {
    let (mut app, _rx) = app();
    agent(&mut app, 0, 0);
    assert_eq!(labels(&app, 0), ["view context"]);
}

#[test]
fn a_shell_only_tab_lists_nothing() {
    let (app, _rx) = app();
    assert_eq!(labels(&app, 0).len(), 0);
}

#[test]
fn a_split_tab_names_each_agent_and_numbers_the_repeats() {
    let (mut app, _rx) = app();
    agent(&mut app, 0, 0);
    let first = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    app.split_pane(first, Dir::Right, AREA, false).expect("a split");
    for term in &mut app.projects[0].workspaces[0].tabs[0].panes {
        term.agent.follow(Some("claude"));
    }
    assert_eq!(labels(&app, 0), ["view context · Claude Code", "view context · Claude Code 2"]);
}

#[test]
fn the_agent_row_menu_puts_it_before_close_agent() {
    let (mut app, _rx) = app();
    let pane = agent(&mut app, 0, 0);
    let actions = app.agent_row_actions(pane, None);
    let at = actions.iter().position(|a| matches!(a, MenuAction::ViewContext { .. })).expect("view context");
    assert!(matches!(actions[at + 1], MenuAction::CloseAgent(_)), "{actions:?}");
}
