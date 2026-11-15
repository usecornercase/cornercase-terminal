use super::a_bug::Attached;
use super::*;
use crate::bots;
use crate::graphics::Support;
use crate::test_util::{TempDir, released, sleeping_as_bots};

const STOP: &str = "a bot cannot stop its own server";

#[test]
fn a_bots_kill_server_and_restart_are_rejected_and_the_server_runs_on() {
    let mut attached = Attached::new();
    let bots_dir = TempDir::new();
    let bin = TempDir::new();
    bots::folder::create(bots_dir.path(), &bots::sample("prs", "PRs"), "").expect("a bot");
    attached.server.app.set_bots_dir(bots_dir.path());
    let children = sleeping_as_bots(bin.path(), &["prs"]);
    let bot = attached.join(Support::default());
    let pid = i32::try_from(children[0].id()).expect("a pid");
    attached.server.app.client_connected(bot.id, Some(pid));

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
    released(bin.path(), children);
}
