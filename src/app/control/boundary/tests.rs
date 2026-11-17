use rstest::rstest;

use super::{Gated, gated, stop_name, window_name};
use crate::control::bots::{BotRequest, BotSay, BotTarget, EditBot, NewBot};
use crate::control::{
    CloneRepository, Command, ConfigImport, Configure, HeldAction, HeldRequest, Input, Keys, NewTab, PasteFile,
    RestartWhenIdle, SendText, Split, TodoSend, UpdateCheck, UpdateInstall,
};
use crate::protocol::ClientMessage;

fn held(action: HeldAction) -> Command {
    Command::Held(HeldRequest { pane: Some(4), tab: None, action })
}

fn input(pane: Option<u64>, popup: bool) -> Command {
    Command::Input(Input { pane, popup, ..Input::default() })
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

const PANE: Option<Gated> = Some(Gated::Pane { pane: Some(4), tab: None });

#[rstest]
#[case::send(Command::Send(SendText { pane: Some(4), ..SendText::default() }), PANE)]
#[case::send_to_a_tab(
    Command::Send(SendText { tab: Some(2), ..SendText::default() }),
    Some(Gated::Pane { pane: None, tab: Some(2) })
)]
#[case::keys(Command::Keys(Keys { pane: Some(4), ..Keys::default() }), PANE)]
#[case::todo_send(Command::TodoSend(TodoSend { id: 1, pane: Some(4), tab: None, force: false }), PANE)]
#[case::held_send_now(held(HeldAction::SendNow), PANE)]
#[case::held_wait_turn(held(HeldAction::WaitTurn), None)]
#[case::input_to_a_popup(input(None, true), Some(Gated::Popup))]
#[case::input_to_a_pane(input(Some(4), false), PANE)]
#[case::input_without_a_pane(input(None, false), None)]
#[case::paste_file(Command::PasteFile(PasteFile { pane: Some(4), ..PasteFile::default() }), PANE)]
#[case::paste_file_without_a_pane(Command::PasteFile(PasteFile::default()), None)]
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
