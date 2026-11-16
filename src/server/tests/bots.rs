use super::a_bug::{Attached, Client, hello};
use super::*;
use crate::bots;
use crate::graphics::Support;
use crate::protocol::Hello;
use crate::test_util::{BotSleepers, TempDir};

struct Joined {
    attached: Attached,
    bot: Client,
    sleepers: BotSleepers,
    _dirs: [TempDir; 2],
}

impl Joined {
    fn new() -> Self {
        let mut attached = Attached::new();
        let (bots_dir, bin) = (TempDir::new(), TempDir::new());
        bots::folder::create(bots_dir.path(), &bots::sample("prs", "PRs"), "").expect("a bot");
        attached.server.app.set_bots_dir(bots_dir.path());
        let sleepers = BotSleepers::start(bin.path(), &["prs"]);
        let bot = attached.join(Support::default());
        attached.server.app.client_connected(bot.id, Some(sleepers.pids()[0]));
        Self { attached, bot, sleepers, _dirs: [bots_dir, bin] }
    }
}

const STOP: &str = "a bot cannot stop its own server";
const WINDOW: &str = "a bot cannot attach as a window";

#[test]
fn a_bots_kill_server_and_restart_are_rejected_and_the_server_runs_on() {
    let Joined { mut attached, bot, sleepers, _dirs } = Joined::new();

    let restart = attached.server.handle(ServerEvent::Message(bot.id, ClientMessage::Restart));
    let kill = attached.server.handle(ServerEvent::Message(bot.id, ClientMessage::KillServer));
    let plain = attached.server.handle(ServerEvent::Message(1, ClientMessage::KillServer));

    assert_eq!((restart, kill), (ControlFlow::Continue(()), ControlFlow::Continue(())));
    assert!(attached.server.restart.is_none());
    assert_eq!(plain, ControlFlow::Break(()));
    let refused = |m: &ServerMessage| matches!(m, ServerMessage::Rejected(why) if why == STOP);
    let mut seen = 0;
    while seen < 2 {
        seen += bot.received(refused).iter().filter(|m| refused(m)).count();
    }
    assert_eq!(seen, 2);
    sleepers.release();
}

#[test]
fn a_stop_queued_behind_the_removal_of_a_bots_connection_is_not_obeyed() {
    let Joined { mut attached, bot, sleepers, _dirs } = Joined::new();
    let other_build = Hello { version: 0, ..hello(HostTheme::default()) };

    let hello = attached.server.handle(ServerEvent::Message(bot.id, ClientMessage::Hello(Box::new(other_build))));
    let kill = attached.server.handle(ServerEvent::Message(bot.id, ClientMessage::KillServer));
    let plain = attached.server.handle(ServerEvent::Message(1, ClientMessage::KillServer));

    assert_eq!((hello, kill, plain), (ControlFlow::Continue(()), ControlFlow::Continue(()), ControlFlow::Break(())));
    sleepers.release();
}

#[rstest::rstest]
#[case::hello(ClientMessage::Hello(Box::new(hello(HostTheme::default()))))]
#[case::input(ClientMessage::Event(Event::Key(KeyCode::Char('x').into())))]
fn a_bots_connection_that_speaks_as_a_window_is_rejected_and_closed(#[case] message: ClientMessage) {
    let Joined { mut attached, bot: _, sleepers, _dirs } = Joined::new();
    let raw = attached.accept();
    attached.server.app.client_connected(raw.id, Some(sleepers.pids()[0]));

    let spoke = attached.server.handle(ServerEvent::Message(raw.id, message));
    let kill = attached.server.handle(ServerEvent::Message(raw.id, ClientMessage::KillServer));

    assert_eq!((spoke, kill), (ControlFlow::Continue(()), ControlFlow::Continue(())));
    let refused = |m: &ServerMessage| matches!(m, ServerMessage::Rejected(why) if why == WINDOW);
    assert!(raw.received(refused).iter().any(refused));
    assert!(attached.server.clients.iter().all(|client| client.id != raw.id));
    sleepers.release();
}

#[test]
fn a_connection_that_is_no_bots_still_attaches_as_a_window() {
    let Joined { mut attached, bot: _, sleepers, _dirs } = Joined::new();
    let raw = attached.accept();

    let _ = attached
        .server
        .handle(ServerEvent::Message(raw.id, ClientMessage::Hello(Box::new(hello(HostTheme::default())))));

    let shown = attached.server.clients.iter().find(|client| client.id == raw.id).and_then(|client| client.size);
    assert!(shown.is_some());
    sleepers.release();
}
