use rstest::rstest;

use super::{Writes, writes};
use crate::control::bots::{BotRequest, BotSay, EditBot, NewBot};
use crate::control::{
    Command, ConfigImport, Configure, HeldAction, HeldRequest, Input, Keys, NewTab, PasteFile, SendText, Split,
    TodoSend,
};

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

const PANE: Option<Writes> = Some(Writes::Pane { pane: Some(4), tab: None });

#[rstest]
#[case::send(Command::Send(SendText { pane: Some(4), ..SendText::default() }), PANE)]
#[case::send_to_a_tab(
    Command::Send(SendText { tab: Some(2), ..SendText::default() }),
    Some(Writes::Pane { pane: None, tab: Some(2) })
)]
#[case::keys(Command::Keys(Keys { pane: Some(4), ..Keys::default() }), PANE)]
#[case::todo_send(Command::TodoSend(TodoSend { id: 1, pane: Some(4), tab: None, force: false }), PANE)]
#[case::held_send_now(held(HeldAction::SendNow), PANE)]
#[case::held_wait_turn(held(HeldAction::WaitTurn), None)]
#[case::input_to_a_popup(input(None, true), Some(Writes::Popup))]
#[case::input_to_a_pane(input(Some(4), false), PANE)]
#[case::input_without_a_pane(input(None, false), None)]
#[case::paste_file(Command::PasteFile(PasteFile { pane: Some(4), ..PasteFile::default() }), PANE)]
#[case::paste_file_without_a_pane(Command::PasteFile(PasteFile::default()), None)]
#[case::new_tab_with_a_command(new_tab(Some("ls")), Some(Writes::NewShell))]
#[case::new_tab_without_a_command(new_tab(None), None)]
#[case::new_tab_with_a_blank_command(new_tab(Some("  ")), None)]
#[case::split_with_a_command(split(Some("ls")), Some(Writes::NewShell))]
#[case::split_without_a_command(split(None), None)]
#[case::a_new_full_bot(new_bot(Some("full")), Some(Writes::Access))]
#[case::a_new_cornercase_bot(new_bot(Some("cornercase")), None)]
#[case::a_new_bot_by_default(new_bot(None), None)]
#[case::editing_the_access(edit_bot(Some("cornercase")), Some(Writes::Access))]
#[case::editing_something_else(edit_bot(None), None)]
#[case::saying_to_a_bot(Command::Bot(BotRequest::Say(BotSay { bot: "x".into(), ..BotSay::default() })), Some(Writes::Says("x".into())))]
#[case::reading_the_settings(Command::Config(Configure::Get), None)]
#[case::searching_the_settings(Command::Config(Configure::Search("a".into())), None)]
#[case::setting_a_setting(Command::Config(Configure::Set(std::collections::BTreeMap::new())), Some(Writes::Settings))]
#[case::removing_a_setting(Command::Config(Configure::Remove("a".into())), Some(Writes::Settings))]
#[case::applying_an_import(import(true), Some(Writes::Settings))]
#[case::previewing_an_import(import(false), None)]
#[case::status(Command::Status(crate::control::Status::default()), None)]
fn a_request_is_classified(#[case] command: Command, #[case] expected: Option<Writes>) {
    assert_eq!(writes(&command), expected);
}
