use super::*;
use crate::agents::Known;
use crate::app::account_menu::AccountItem;
use crate::app::help_menu::HelpItem;
use crate::app::menus::Marked;
use crate::app::opening::Subject;
use crate::app::plugins::Spot;
use crate::host::open::Opener;
use crate::messenger::Via;
use crate::project::split::{Dir, Place};
use crate::shortcuts::Action;

fn every_action() -> Vec<Action> {
    vec![
        Action::NextTab,
        Action::PreviousTab,
        Action::Tab(0),
        Action::NextWorkspace,
        Action::PreviousWorkspace,
        Action::NextProject,
        Action::PreviousProject,
        Action::Pane(Place::Left),
        Action::Agent,
        Action::Mark,
        Action::Search,
        Action::FindSaid,
        Action::NewTab,
        Action::Split(Dir::Right),
        Action::ClosePane,
        Action::RenameTab,
        Action::FindNames,
        Action::FindText,
        Action::Files,
        Action::Changes,
        Action::Base,
        Action::PullRequest,
        Action::Flow,
        Action::NewWorkspace,
        Action::StartFlow,
        Action::RenameWorkspace,
        Action::CloseWorkspace,
        Action::CloseAgents,
        Action::ReopenAgents,
        Action::Issues,
        Action::Todo,
        Action::Settings,
        Action::Usage,
        Action::Away,
        Action::Quit,
        Action::Help,
    ]
}

fn action_index(action: &Action) -> usize {
    match action {
        Action::NextTab => 0,
        Action::PreviousTab => 1,
        Action::Tab(_) => 2,
        Action::NextWorkspace => 3,
        Action::PreviousWorkspace => 4,
        Action::NextProject => 5,
        Action::PreviousProject => 6,
        Action::Pane(_) => 7,
        Action::Agent => 8,
        Action::Mark => 9,
        Action::Search => 10,
        Action::FindSaid => 11,
        Action::NewTab => 12,
        Action::Split(_) => 13,
        Action::ClosePane => 14,
        Action::RenameTab => 15,
        Action::FindNames => 16,
        Action::FindText => 17,
        Action::Files => 18,
        Action::Changes => 19,
        Action::Base => 20,
        Action::PullRequest => 21,
        Action::Flow => 22,
        Action::NewWorkspace => 23,
        Action::StartFlow => 24,
        Action::RenameWorkspace => 25,
        Action::CloseWorkspace => 26,
        Action::CloseAgents => 27,
        Action::ReopenAgents => 28,
        Action::Issues => 29,
        Action::Todo => 30,
        Action::Settings => 31,
        Action::Usage => 32,
        Action::Away => 33,
        Action::Quit => 34,
        Action::Help => 35,
    }
}

#[test]
fn every_shortcut_is_answered_by_a_stage_or_the_last_match() {
    let actions = every_action();
    assert_eq!(actions.iter().map(action_index).collect::<Vec<_>>(), (0..actions.len()).collect::<Vec<_>>());
    let mut left = Vec::new();
    for action in actions {
        let (mut app, _rx) = app();
        let rest = app
            .go_shortcut(action, AREA)
            .and_then(|action| app.panel_shortcut(action))
            .and_then(|action| app.edit_shortcut(action));
        left.extend(rest);
    }
    assert_eq!(
        left,
        [Action::NewTab, Action::Split(Dir::Right), Action::CloseAgents, Action::ReopenAgents, Action::Issues]
    );
}

fn every_menu_action() -> Vec<MenuAction> {
    let (subject, kind) = (Subject::Project(1), Known::Claude);
    vec![
        MenuAction::Rename(Target::Group(1)),
        MenuAction::MoveToGroup(1),
        MenuAction::SetGroup(1, None),
        MenuAction::GroupStyle(1),
        MenuAction::DeleteGroup(1),
        MenuAction::AddProject(1),
        MenuAction::OpenProject,
        MenuAction::NewGroup,
        MenuAction::Pane(1, PaneAction::Close),
        MenuAction::Mark(Marked::Pane(1), true),
        MenuAction::AgentFold(1, true),
        MenuAction::PullRequest(1),
        MenuAction::ShowFlow(1),
        MenuAction::Browser,
        MenuAction::Away(Via::Telegram),
        MenuAction::NewWorkspace(1),
        MenuAction::StartFlow(1),
        MenuAction::ProjectSettings(1),
        MenuAction::RunSetup(1),
        MenuAction::OpenIn(subject),
        MenuAction::OpenWith(subject, Opener::Default),
        MenuAction::CopyPath(subject),
        MenuAction::SendTodo { todo: 1, pane: 1 },
        MenuAction::Plugin { spot: Spot::Pane(1), index: 0, revision: 0 },
        MenuAction::Account(AccountItem { pane: 1, generation: 0, kind, index: 0 }, (0, 0)),
        MenuAction::ManageAccounts,
        MenuAction::Worktrees(1),
        MenuAction::CloseAgent(1),
        MenuAction::AccountNote(AccountItem { pane: 1, generation: 0, kind, index: 0 }),
        MenuAction::AccountsOf(AccountItem { pane: 1, generation: 0, kind, index: 0 }),
        MenuAction::AccountMenu(1),
        MenuAction::CloseAgents(1),
        MenuAction::ReopenAgents(1),
        MenuAction::ForgetAgents(1),
        MenuAction::Help(HelpItem::Docs),
        MenuAction::Tour(0),
        MenuAction::PluginButton { plugin: 0, revision: 0 },
    ]
}

fn menu_index(action: &MenuAction) -> usize {
    match action {
        MenuAction::Rename(_) => 0,
        MenuAction::MoveToGroup(_) => 1,
        MenuAction::SetGroup(..) => 2,
        MenuAction::GroupStyle(_) => 3,
        MenuAction::DeleteGroup(_) => 4,
        MenuAction::AddProject(_) => 5,
        MenuAction::OpenProject => 6,
        MenuAction::NewGroup => 7,
        MenuAction::Pane(..) => 8,
        MenuAction::Mark(..) => 9,
        MenuAction::AgentFold(..) => 10,
        MenuAction::PullRequest(_) => 11,
        MenuAction::ShowFlow(_) => 12,
        MenuAction::Browser => 13,
        MenuAction::Away(_) => 14,
        MenuAction::NewWorkspace(_) => 15,
        MenuAction::StartFlow(_) => 16,
        MenuAction::ProjectSettings(_) => 17,
        MenuAction::RunSetup(_) => 18,
        MenuAction::OpenIn(_) => 19,
        MenuAction::OpenWith(..) => 20,
        MenuAction::CopyPath(_) => 21,
        MenuAction::SendTodo { .. } => 22,
        MenuAction::Plugin { .. } => 23,
        MenuAction::Account(..) => 24,
        MenuAction::ManageAccounts => 25,
        MenuAction::Worktrees(_) => 26,
        MenuAction::CloseAgent(_) => 27,
        MenuAction::AccountNote(_) => 28,
        MenuAction::AccountsOf(_) => 29,
        MenuAction::AccountMenu(_) => 30,
        MenuAction::CloseAgents(_) => 31,
        MenuAction::ReopenAgents(_) => 32,
        MenuAction::ForgetAgents(_) => 33,
        MenuAction::Help(_) => 34,
        MenuAction::Tour(_) => 35,
        MenuAction::PluginButton { .. } => 36,
    }
}

#[test]
fn every_menu_action_is_answered_by_a_stage_or_the_last_match() {
    let actions = every_menu_action();
    assert_eq!(actions.iter().map(menu_index).collect::<Vec<_>>(), (0..actions.len()).collect::<Vec<_>>());
    let at = Position::new(0, 0);
    let mut left = Vec::new();
    for action in actions {
        let (mut app, _rx) = app();
        let rest = app
            .group_action(action, at)
            .and_then(|action| app.project_action(action, at, AREA))
            .and_then(|action| app.agent_action(action));
        left.extend(rest);
    }
    assert_eq!(left, [MenuAction::Pane(1, PaneAction::Close)]);
}

#[test]
fn every_command_is_answered_by_an_area() {
    let commands = crate::control::tests::every_command();
    assert_eq!(commands.iter().map(crate::control::Command::name).collect::<Vec<_>>(), crate::control::Command::NAMES);
    for command in commands {
        let name = command.name();
        let (mut app, _rx) = app();
        let text = serde_json::to_string(&crate::control::Request {
            caller: None,
            server: None,
            contract: None,
            boot: None,
            view: None,
            command,
        })
        .expect("json");
        app.request(9, &text, Some(AREA), Instant::now());
        for (_, answer) in app.take_answers() {
            assert!(!answer.contains("no area answers"), "{name}: {answer}");
        }
    }
}
