use super::*;
use crate::control::contract::Code;
use crate::control::{OpenView, SetView};

const CLIENT: u64 = 50;

fn named(bot: &str) -> OpenView {
    OpenView { bot: Some(bot.into()), ..OpenView::default() }
}

fn open(c: &mut Chat, view: &OpenView) -> u64 {
    let opened = c.app.view_request(CLIENT, view, Some(AREA)).expect("open a view").expect("a snapshot");
    opened["view"]["id"].as_u64().expect("a view id")
}

fn set(c: &mut Chat, id: u64, set: &SetView) -> serde_json::Value {
    c.app.view_set_request(id, set, Some(AREA), Instant::now()).expect("set").expect("an answer")
}

fn bot_set(bot: &str) -> SetView {
    SetView { bot: Some(bot.into()), ..SetView::default() }
}

fn answered(c: &mut Chat) {
    c.say("hi");
    c.settle();
}

#[test]
fn an_answer_a_visible_view_shows_is_neither_unread_nor_notified() {
    let mut c = chat(ANSWER);
    open(&mut c, &named("prs"));

    answered(&mut c);

    assert_eq!(c.app.panels.bots.live[0].unread, 0);
    assert!(c.app.messages.toast.is_none());
    assert_eq!(c.app.outbox.take_notifications().len(), 0);
}

#[rstest]
#[case::hidden(OpenView { visible: Some(false), ..named("prs") })]
#[case::no_bot(OpenView::default())]
fn a_hidden_view_or_one_without_the_bot_leaves_the_answer_unread_and_told(#[case] view: OpenView) {
    let mut c = chat(ANSWER);
    open(&mut c, &view);

    answered(&mut c);

    assert_eq!(c.app.panels.bots.live[0].unread, 1);
    assert!(c.app.messages.toast.as_ref().is_some_and(|toast| toast.go.is_some()));
    assert_eq!(c.app.outbox.take_notifications().len(), 1);
}

#[test]
fn a_bot_named_by_its_name_is_stored_by_id_and_marked_seen() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.live[0].unread = 2;

    let id = open(&mut c, &named("PRs"));

    assert_eq!(c.app.view_report(id).and_then(|report| report.view).and_then(|view| view.bot).as_deref(), Some("prs"));
    assert_eq!(c.app.panels.bots.live[0].unread, 0);
}

#[test]
fn an_unknown_bot_is_not_found_and_opens_no_view() {
    let mut c = chat(ANSWER);

    let refused = c.app.view_request(CLIENT, &named("nobody"), Some(AREA)).expect_err("refused");

    assert_eq!(refused.code, Code::NotFound);
    assert!(c.app.clients.views.is_empty());
}

#[test]
fn view_set_names_clears_and_sees_the_bot() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.live[0].unread = 2;
    let id = open(&mut c, &OpenView { visible: Some(false), ..named("prs") });
    assert_eq!(c.app.panels.bots.live[0].unread, 2);

    let unknown = c.app.view_set_request(id, &bot_set("nobody"), Some(AREA), Instant::now()).expect_err("refused");
    assert_eq!(unknown.code, Code::NotFound);
    let back = set(&mut c, id, &SetView { visible: Some(true), ..SetView::default() });
    assert_eq!((back["bot"].as_str(), c.app.panels.bots.live[0].unread), (Some("prs"), 0));

    let cleared = set(&mut c, id, &bot_set(""));
    assert!(cleared.get("bot").is_none());
    answered(&mut c);
    assert_eq!(c.app.panels.bots.live[0].unread, 1);
}

#[test]
fn the_view_ending_releases_the_bot() {
    let mut c = chat(ANSWER);
    open(&mut c, &named("prs"));

    c.app.forget_views(CLIENT);
    answered(&mut c);

    assert_eq!(c.app.panels.bots.live[0].unread, 1);
    assert!(c.app.messages.toast.is_some());
}

#[test]
fn removing_the_bot_takes_it_off_the_view() {
    let mut c = chat(ANSWER);
    let id = open(&mut c, &named("prs"));

    c.app.bot_remove("prs").expect("remove");

    let info = c.app.view_report(id).and_then(|report| report.view).expect("the view");
    assert_eq!(info.bot, None);
    assert!(c.app.clients.views.changed());
}

#[cfg(unix)]
#[test]
fn a_view_that_shows_a_bot_does_not_see_its_tab_and_keeps_sizing_it() {
    let mut c = chat(ANSWER);
    c.app.open_here(AREA).expect("open the first project");
    let tab = c.app.projects[0].workspaces[0].tabs[0].id;
    let sized = OpenView { cols: Some(80), rows: Some(24), tab: Some(tab), ..OpenView::default() };
    let id = open(&mut c, &sized);
    assert_eq!(c.app.seen_tabs(), vec![tab]);
    let before = c.app.view_projection(id);

    let showing = set(&mut c, id, &bot_set("prs"));
    assert_eq!(c.app.seen_tabs().len(), 0);
    assert_eq!((showing["controls"].as_bool(), c.app.view_projection(id)), (Some(true), before));

    set(&mut c, id, &bot_set(""));
    assert_eq!(c.app.seen_tabs(), vec![tab]);
}

#[cfg(unix)]
#[test]
fn a_view_that_does_not_claim_and_names_a_bot_does_not_see_its_tab() {
    let mut c = chat(ANSWER);
    c.app.open_here(AREA).expect("open the first project");
    let tab = c.app.projects[0].workspaces[0].tabs[0].id;
    let watching = OpenView { cols: Some(80), rows: Some(24), tab: Some(tab), no_claim: true, ..named("prs") };

    let id = open(&mut c, &watching);

    let info = c.app.view_report(id).and_then(|report| report.view).expect("the view");
    assert_eq!((info.claims, info.controls), (false, false));
    assert_eq!(c.app.seen_tabs().len(), 0);
}
