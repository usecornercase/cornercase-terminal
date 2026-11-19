use super::*;
use crate::host::clipboard;
use crate::host::open::{Link, Opener, link_host};

const DOCS: &str = "https://example.com/docs";
const NOTHING: [(u64, Link); 0] = [];

fn answered(text: &str) -> Chat {
    let mut c = chat(ANSWER);
    let path = c.app.panels.bots.live[0].folder.join(folder::CHAT_FILE);
    let id = c.app.panels.bots.live[0].next_line;
    chat::append(&path, &Line::new(id, 100, chat::Kind::Bot, text)).expect("append");
    c.app.panels.bots.live[0].next_line += 1;
    c.app.window_attached();
    let row = {
        let a = c.app.layout(AREA);
        ui::bot_rows(a.bots_list, a.pitch, 1, 0).item(0)
    };
    render(&mut c.app, AREA);
    click(&mut c.app, Position::new(row.x + 3, row.y));
    c
}

fn link_at(app: &mut App) -> (Position, String) {
    match chat_spot(app, |hit| matches!(hit, ChatHit::Link(_))) {
        Some((pos, ChatHit::Link(url))) => (pos, url),
        _ => panic!("no link in the chat"),
    }
}

fn click_link(c: &mut Chat, openers: &[Opener]) {
    c.app.set_window(7, openers);
    let (pos, _) = link_at(&mut c.app);
    click(&mut c.app, pos);
}

fn dialog(c: &mut Chat) -> Option<String> {
    match &c.app.overlay {
        Some(Overlay::OpenLink { url, .. }) => Some(url.clone()),
        _ => None,
    }
}

fn press_button(c: &mut Chat, label: &str) {
    render(&mut c.app, AREA);
    let form = ui::form_area(AREA);
    let rect = if label == "copy" {
        ui::form_extra_button(form, "open", "copy")
    } else {
        let [open, cancel] = ui::form_buttons(form, "open");
        if label == "open" { open } else { cancel }
    };
    click(&mut c.app, Position::new(rect.x + 1, rect.y));
}

fn drawn_dialog(app: &mut App) -> String {
    drawn_dialog_in(app, AREA)
}

fn drawn_dialog_in(app: &mut App, area: Rect) -> String {
    let screen = render(app, area);
    let buffer = screen.backend().buffer();
    let form = ui::form_area(area);
    let row = |y| (form.x..form.right()).map(|x| buffer[(x, y)].symbol()).collect::<String>();
    (form.y..form.bottom()).map(row).collect::<Vec<_>>().join("\n")
}

fn sent(c: &mut Chat) -> Vec<(u64, Link)> {
    let links = c.app.take_links();
    assert!(links.iter().all(|(_, link)| link_host(&link.url).is_ok()), "{links:?}");
    links
}

#[test]
fn a_click_on_an_address_asks_first_in_a_window_that_can_browse() {
    let mut c = answered(&format!("read {DOCS} first"));
    click_link(&mut c, &[Opener::Default]);
    assert_eq!(dialog(&mut c).as_deref(), Some(DOCS));
    assert_eq!(sent(&mut c), NOTHING);
    insta::assert_snapshot!(drawn_dialog(&mut c.app));
}

#[test]
fn open_tells_the_window_that_clicked() {
    let mut c = answered(DOCS);
    click_link(&mut c, &[Opener::Default]);
    press_button(&mut c, "open");
    assert_eq!(sent(&mut c), [(7, Link { url: DOCS.into() })]);
    assert!(c.app.overlay.is_none());
}

#[test]
fn enter_opens_too() {
    let mut c = answered(DOCS);
    click_link(&mut c, &[Opener::Default]);
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(sent(&mut c), [(7, Link { url: DOCS.into() })]);
    assert!(c.app.overlay.is_none());
}

#[test]
fn copy_copies_the_address_and_opens_nothing() {
    let mut c = answered(DOCS);
    click_link(&mut c, &[Opener::Default]);
    c.app.take_host_writes();
    press_button(&mut c, "copy");
    assert_eq!(c.app.take_host_writes(), [clipboard::osc52(DOCS)]);
    assert_eq!(sent(&mut c), NOTHING);
    assert!(c.app.overlay.is_none());
}

#[rstest]
#[case::by_escape(true)]
#[case::by_cancel(false)]
fn cancelling_opens_and_copies_nothing(#[case] escape: bool) {
    let mut c = answered(DOCS);
    click_link(&mut c, &[Opener::Default]);
    c.app.take_host_writes();
    if escape {
        send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    } else {
        press_button(&mut c, "cancel");
    }
    assert!(c.app.overlay.is_none());
    assert_eq!(sent(&mut c), NOTHING);
    assert_eq!(c.app.take_host_writes(), Vec::<Vec<u8>>::new());
}

#[test]
fn a_window_that_cannot_browse_copies_at_once() {
    let mut c = answered(DOCS);
    c.app.take_host_writes();
    click_link(&mut c, &[Opener::Files]);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.take_host_writes(), [clipboard::osc52(DOCS)]);
    assert_eq!(toast(&c.app), Some(COPIED));
    assert_eq!(sent(&mut c), NOTHING);
}

#[test]
fn a_dialog_submitted_from_a_window_that_cannot_browse_copies() {
    let mut c = answered(DOCS);
    click_link(&mut c, &[Opener::Default]);
    c.app.set_window(8, &[]);
    c.app.take_host_writes();
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(sent(&mut c), NOTHING);
    assert_eq!(c.app.take_host_writes(), [clipboard::osc52(DOCS)]);
}

#[test]
fn an_address_with_a_user_name_is_copied_and_the_toast_says_why() {
    let mut c = answered("log in at https://good.com@evil.example/ now");
    c.app.take_host_writes();
    click_link(&mut c, &[Opener::Default]);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.take_host_writes(), [clipboard::osc52("https://good.com@evil.example/")]);
    assert_eq!(toast(&c.app), Some("copied the address: it carries a user name, so it is not opened"));
    assert_eq!(sent(&mut c), NOTHING);
}

#[test]
fn an_address_cut_across_rows_opens_whole() {
    let long = format!("https://example.com/{}", "segment/".repeat(30));
    let mut c = answered(&format!("see {long}"));
    c.app.set_window(7, &[Opener::Default]);
    let (pos, drawn) = link_at(&mut c.app);
    assert!(drawn.len() < long.len(), "{drawn}");
    click(&mut c.app, pos);
    assert_eq!(dialog(&mut c), Some(long.clone()));
    insta::assert_snapshot!(drawn_dialog(&mut c.app));
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(sent(&mut c), [(7, Link { url: long })]);
}

#[test]
fn removing_the_bot_closes_the_dialog() {
    let mut c = answered(DOCS);
    click_link(&mut c, &[Opener::Default]);
    assert!(c.app.bot_remove("prs").is_ok());
    assert!(c.app.overlay.is_none());
}

#[test]
fn a_narrow_window_still_shows_where_a_long_address_is_cut() {
    let long = format!("https://example.com/{}", "segment/".repeat(30));
    let mut c = answered(&format!("see {long}"));
    c.app.set_window(7, &[Opener::Default]);
    let (pos, _) = link_at(&mut c.app);
    click(&mut c.app, pos);
    insta::assert_snapshot!(drawn_dialog_in(&mut c.app, Rect::new(0, 0, 40, 20)));
}
