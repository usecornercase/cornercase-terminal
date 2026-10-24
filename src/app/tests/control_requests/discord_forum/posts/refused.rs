use super::*;
use crate::discord::forum::Server;
use crate::discord::gateway::DIRECT_ONLY;

const CONTENT: &str = "turn on Message Content Intent: Developer Portal › Bot";

fn refused() -> Posting {
    let mut posting = Posting::new();
    posting.link(LinkEvent::DirectOnly(1));
    posting
}

fn read(posting: &mut Posting, content: bool) {
    let servers = vec![Server { id: 7, name: "home".into() }];
    let progress = Progress { checklist: Checklist { guild_install: true, content, servers }, forum: None };
    let read = MessengerEvent::Forum { via: Via::Discord, token: Secret(TOKEN.into()), read: Ok(progress) };
    posting.agent.app.handle_event(AppEvent::Messenger(read), AREA).expect("the forum's event is handled");
}

#[test]
fn a_refused_content_intent_says_so_in_the_forum_row_and_not_in_the_bot_row() {
    let posting = refused();

    let settings = posting.agent.app.messenger_settings(Via::Discord);

    assert_eq!((settings.forum.as_str(), settings.forum_ready), (DIRECT_ONLY, false));
    assert_ne!(settings.note, DIRECT_ONLY);
}

#[test]
fn a_recovered_connection_keeps_the_note() {
    let mut posting = refused();

    posting.link(LinkEvent::Problem(1, None));

    assert_eq!(posting.agent.app.messenger_settings(Via::Discord).forum, DIRECT_ONLY);
}

#[test]
fn the_socket_names_the_intent_as_the_pending_step() {
    let posting = refused();

    let forum = posting.agent.app.forum_info(Via::Discord).expect("the forum");

    assert_eq!((forum.ready, forum.pending.as_deref()), (true, Some(CONTENT)));
}

#[rstest]
#[case::the_intent_is_off(false)]
#[case::the_intent_was_refused_since(true)]
fn the_dialog_shows_the_intent_step_pending_when(#[case] content: bool) {
    let mut posting = refused();
    posting.agent.app.open_forum(Via::Discord);

    read(&mut posting, content);

    let steps = posting.agent.app.forum_view(Via::Discord).steps;
    assert_eq!(steps.iter().map(|(done, _)| *done).collect::<Vec<_>>(), [true, false, true, true]);
    assert_eq!(steps[1].1, CONTENT);
}

#[test]
fn setting_the_forum_up_again_clears_the_note() {
    let mut posting = refused();

    forum_set_up_again(&mut posting, FORUM);

    let settings = posting.agent.app.messenger_settings(Via::Discord);
    assert_eq!((settings.forum.as_str(), settings.forum_ready), ("home › #cornercase", true));
}
