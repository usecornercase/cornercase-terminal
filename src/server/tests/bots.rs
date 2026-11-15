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
