use rstest::rstest;

use super::table::{CLOSED, OPEN, RULED, closed};
use super::typing::not_a_prompt;
use super::{Gated, Text, gated, stop_name, window_name};
use crate::agents::Known;
use crate::control::bots::{BotRequest, BotSay, BotTarget, EditBot, NewBot};
use crate::control::{
    AgentsClose, CloneRepository, Close, CloseAgent, Command, ConfigImport, Configure, FlowImport, FlowSave,
    HeldAction, HeldRequest, Item, MachineSet, Machines, NewTab, Open, OpenView, PasteFile, RestartWhenIdle, SendText,
    SetView, Split, TodoSend, UpdateCheck, UpdateInstall, WorktreeSize,
};
use crate::protocol::ClientMessage;

fn held(action: HeldAction) -> Command {
    Command::Held(HeldRequest { pane: Some(4), tab: None, action })
}

fn new_tab(command: Option<&str>) -> Command {
    Command::NewTab(NewTab { command: command.map(str::to_string), ..NewTab::default() })
}

fn split(command: Option<&str>) -> Command {
    Command::Split(Split { command: command.map(str::to_string), ..Split::default() })
}

fn new_bot(access: Option<&str>) -> Command {
    Command::Bot(BotRequest::New(NewBot { name: "x".into(), access: access.map(str::to_string), ..NewBot::default() }))
}

fn edit_bot(access: Option<&str>) -> Command {
    Command::Bot(BotRequest::Edit(EditBot {
        bot: "x".into(),
        access: access.map(str::to_string),
        ..EditBot::default()
    }))
}

fn import(apply: bool) -> Command {
    Command::ConfigImport(ConfigImport { apply, ..ConfigImport::default() })
}

const PANE: Option<Gated> = Some(Gated::Pane { pane: Some(4), tab: None, text: Text::Kept });

fn send(text: Option<&str>) -> Command {
    Command::Send(SendText { pane: Some(4), text: text.map(str::to_string), ..SendText::default() })
}

fn close(item: Item) -> Command {
    Command::Close(Close { item, remove_worktree: false, force: false, unlock: false })
}

fn close_agent(check: bool) -> Command {
    Command::CloseAgent(CloseAgent { pane: 4, check, ..CloseAgent::default() })
}

fn agents_close(check: bool) -> Command {
    Command::AgentsClose(AgentsClose { workspace: Some(1), check, force: false })
}

fn flow_import(check: bool) -> Command {
    Command::FlowImport(FlowImport { check, ..FlowImport::default() })
}

fn flow_save(check: bool) -> Command {
    Command::FlowSave(FlowSave { check, ..FlowSave::default() })
}

fn typed(text: &str) -> Gated {
    Gated::Pane { pane: Some(4), tab: None, text: Text::Given(text.into()) }
}

#[rstest]
#[case::send(send(Some("hi")), Some(typed("hi")))]
#[case::send_without_a_text(send(None), Some(Gated::Pane { pane: Some(4), tab: None, text: Text::Missing }))]
#[case::send_with_an_empty_text(send(Some("")), Some(Gated::Pane { pane: Some(4), tab: None, text: Text::Missing }))]
#[case::send_to_a_tab(
    Command::Send(SendText { tab: Some(2), ..SendText::default() }),
    Some(Gated::Pane { pane: None, tab: Some(2), text: Text::Missing })
)]
#[case::todo_send(
    Command::TodoSend(TodoSend { id: 1, pane: Some(4), tab: None, force: false }),
    Some(Gated::Pane { pane: Some(4), tab: None, text: Text::Todo(1) })
)]
#[case::held_send_now(held(HeldAction::SendNow), PANE)]
#[case::held_wait_turn(held(HeldAction::WaitTurn), None)]
#[case::paste_file(Command::PasteFile(PasteFile { pane: Some(4), ..PasteFile::default() }), PANE)]
#[case::paste_file_without_a_pane(Command::PasteFile(PasteFile::default()), None)]
#[case::open(Command::Open(Open { path: "/x".into(), ..Open::default() }), Some(Gated::Opens("/x".into())))]
#[case::worktree_size_of_a_path(
    Command::WorktreeSize(WorktreeSize { workspace: None, path: Some("/x".into()) }),
    Some(Gated::Measures("/x".into()))
)]
#[case::worktree_size_of_a_workspace(Command::WorktreeSize(WorktreeSize { workspace: Some(1), path: None }), None)]
#[case::close_a_pane(close(Item::Pane(4)), Some(Gated::Closes(Item::Pane(4))))]
#[case::close_a_workspace(close(Item::Workspace(2)), Some(Gated::Closes(Item::Workspace(2))))]
#[case::close_an_agent(close_agent(false), Some(Gated::Closing))]
#[case::check_closing_an_agent(close_agent(true), None)]
#[case::close_agents(agents_close(false), Some(Gated::Closing))]
#[case::check_closing_agents(agents_close(true), None)]
#[case::import_a_flow(flow_import(false), Some(Gated::Flows))]
#[case::preview_a_flow_import(flow_import(true), None)]
#[case::save_a_flow(flow_save(false), Some(Gated::Flows))]
#[case::check_a_flow(flow_save(true), None)]
#[case::list_machines(Command::Machines(Machines::List), None)]
#[case::retry_machines(Command::Machines(Machines::Retry(None)), None)]
#[case::set_a_machine(Command::Machines(Machines::Set(MachineSet::default())), Some(Gated::Machines))]
#[case::remove_a_machine(Command::Machines(Machines::Remove("x".into())), Some(Gated::Machines))]
#[case::update_a_machine(Command::Machines(Machines::Update("x".into())), Some(Gated::Machines))]
#[case::new_tab_with_a_command(new_tab(Some("ls")), Some(Gated::NewShell))]
#[case::new_tab_without_a_command(new_tab(None), None)]
#[case::new_tab_with_a_blank_command(new_tab(Some("  ")), None)]
#[case::split_with_a_command(split(Some("ls")), Some(Gated::NewShell))]
#[case::split_without_a_command(split(None), None)]
#[case::a_new_full_bot(new_bot(Some("full")), Some(Gated::Access))]
#[case::a_new_cornercase_bot(new_bot(Some("cornercase")), None)]
#[case::a_new_bot_by_default(new_bot(None), None)]
#[case::editing_the_access(edit_bot(Some("cornercase")), Some(Gated::Access))]
#[case::editing_something_else(edit_bot(None), Some(Gated::Says("x".into())))]
#[case::reading_what_a_bot_is_for(Command::Bot(BotRequest::About(BotTarget { bot: "x".into() })), None)]
#[case::setting_a_bot_up(Command::Bot(BotRequest::Setup(BotTarget { bot: "x".into() })), Some(Gated::Says("x".into())))]
#[case::saying_to_a_bot(Command::Bot(BotRequest::Say(BotSay { bot: "x".into(), ..BotSay::default() })), Some(Gated::Says("x".into())))]
#[case::a_view_that_shows_a_bot(Command::View(OpenView { bot: Some("x".into()), ..OpenView::default() }), Some(Gated::ShowsChat))]
#[case::a_view_with_no_bot(Command::View(OpenView::default()), None)]
#[case::a_view_set_to_a_bot(Command::ViewSet(SetView { bot: Some("x".into()), ..SetView::default() }), Some(Gated::ShowsChat))]
#[case::a_view_set_to_no_bot(Command::ViewSet(SetView { bot: Some(String::new()), ..SetView::default() }), None)]
#[case::a_view_set_made_visible(Command::ViewSet(SetView { visible: Some(true), ..SetView::default() }), Some(Gated::Reveals))]
#[case::a_view_set_made_visible_with_no_bot(Command::ViewSet(SetView { visible: Some(true), bot: Some(String::new()), ..SetView::default() }), None)]
#[case::a_view_set_made_hidden(Command::ViewSet(SetView { visible: Some(false), ..SetView::default() }), None)]
#[case::reading_the_settings(Command::Config(Configure::Get), None)]
#[case::searching_the_settings(Command::Config(Configure::Search("a".into())), None)]
#[case::setting_a_setting(Command::Config(Configure::Set(std::collections::BTreeMap::new())), Some(Gated::Settings))]
#[case::removing_a_setting(Command::Config(Configure::Remove("a".into())), Some(Gated::Settings))]
#[case::applying_an_import(import(true), Some(Gated::Settings))]
#[case::previewing_an_import(import(false), None)]
#[case::restart_when_idle(Command::RestartWhenIdle(RestartWhenIdle::default()), Some(Gated::ServerStop))]
#[case::update_install(Command::UpdateInstall(UpdateInstall::default()), Some(Gated::ServerStop))]
#[case::update_check(Command::UpdateCheck(UpdateCheck::default()), None)]
#[case::clone(Command::Clone(CloneRepository::default()), Some(Gated::Clone))]
#[case::status(Command::Status(crate::control::Status::default()), None)]
fn a_request_is_classified(#[case] command: Command, #[case] expected: Option<Gated>) {
    assert_eq!(gated(&command), expected);
}

#[test]
fn every_request_is_decided_once() {
    let closed: Vec<&str> = CLOSED.iter().flat_map(|(names, _)| names.iter().copied()).collect();
    let lists = [closed.as_slice(), RULED, OPEN];
    for name in Command::NAMES {
        let found = lists.iter().filter(|list| list.contains(name)).count();
        assert_eq!(found, 1, "`{name}` must be in exactly one of CLOSED, RULED and OPEN");
    }
    for name in lists.concat() {
        assert!(Command::NAMES.contains(&name), "`{name}` is not a request");
    }
}

#[rstest]
#[case::keys("keys", "a bot cannot press keys")]
#[case::flow_run("flow-run", "a bot cannot run a flow")]
#[case::flows("flow-delete", "a bot cannot change cornercase's flows")]
#[case::plugins("plugin-run", "a bot cannot run or change cornercase's plugins")]
#[case::stop("stop-port", "a bot cannot stop a process")]
#[case::discard("changes-discard", "a bot cannot discard changes")]
#[case::worktrees("free-builds", "a bot cannot remove a worktree")]
#[case::own("agents-forget", "a bot closes only the tabs and panes it started")]
#[case::accounts("profile-use", "a bot cannot change your accounts")]
#[case::pair("config-pair", "a bot cannot pair a device")]
#[case::forum("forum-server", "a bot cannot change cornercase's settings")]
#[case::tunnels("tunnel-hold", "a bot cannot open or close a tunnel")]
fn a_closed_request_says_why(#[case] name: &str, #[case] starts: &str) {
    let why = closed(name).expect("closed");
    assert!(why.starts_with(starts), "{why}");
}

#[test]
fn an_open_request_is_not_closed() {
    assert_eq!(closed("status"), None);
    assert_eq!(closed("send"), None);
}

#[rstest]
#[case::a_command("/exit", None, true)]
#[case::shell_mode_after_spaces("  !ls", None, true)]
#[case::a_command_on_the_next_line("\n/x", None, true)]
#[case::a_path_inside("fix /usr/bin", None, false)]
#[case::a_heading("# heading", None, false)]
#[case::an_exclamation("hello!", None, false)]
#[case::copilots_help("?", Some(Known::Copilot), true)]
#[case::a_question_for_claude("?", Some(Known::Claude), false)]
#[case::lines_and_tabs("fix this\n\tand that\r\n", None, false)]
#[case::an_escape_that_ends_the_paste("\x1b[201\x1b[201~~\x1b[Z", None, true)]
#[case::a_lone_escape("go\x1b", None, true)]
#[case::a_c1_control("go\u{9b}Z", None, true)]
#[case::a_control_letter("go\x03", None, true)]
fn a_prompt_is_told_from_an_agents_command(#[case] text: &str, #[case] agent: Option<Known>, #[case] refused: bool) {
    assert_eq!(not_a_prompt(text, agent).is_some(), refused);
}

#[test]
fn a_text_with_keys_in_it_says_so() {
    let why = not_a_prompt("go\x1b[Z", None).expect("refused");
    assert!(why.starts_with("a bot writes prompts to an agent, not keys"), "{why}");
    assert!(not_a_prompt("/exit", None).is_some_and(|why| why.contains("`/` or `!`")));
}

#[test]
fn every_stop_is_a_request_or_a_message_the_server_knows() {
    let messages = [stop_name(&ClientMessage::KillServer), stop_name(&ClientMessage::Restart)];
    for stop in crate::bots::SERVER_STOPS {
        for asked in stop.asks {
            assert!(Command::NAMES.contains(asked) || messages.contains(&Some(asked)), "{asked}");
        }
    }
}

#[test]
fn every_message_but_a_request_and_a_stop_is_a_window_message() {
    let failed = crate::host::open::Failed { opener: crate::host::open::Opener::Finder, reason: String::new() };
    let windows = [
        ClientMessage::Event(crossterm::event::Event::FocusGained),
        ClientMessage::Graphics(crate::graphics::Support::default()),
        ClientMessage::Activated("t".into()),
        ClientMessage::OpenFailed(failed),
        ClientMessage::Machines(crate::protocol::Machines::default()),
        ClientMessage::Tunnels(crate::protocol::Holding::default()),
    ];
    for message in &windows {
        assert!(window_name(message).is_some(), "{message:?}");
    }
    for message in [ClientMessage::Request("{}".into()), ClientMessage::KillServer, ClientMessage::Restart] {
        assert_eq!(window_name(&message), None, "{message:?}");
    }
}
