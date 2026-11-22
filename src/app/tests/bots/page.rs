use super::*;

const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };
const TALL: Rect = Rect { x: 0, y: 0, width: 120, height: 40 };

fn drawn(app: &mut App) -> String {
    format!("{}", render(app, AREA).backend())
}

fn drawn_tall(app: &mut App) -> String {
    format!("{}", render(app, TALL).backend())
}

fn press_button(app: &mut App, area: Rect) {
    render(app, area);
    let button = app.layout(area).shown(app.screen.nav).bots_button;
    assert!(!button.is_empty(), "the Bots button is there");
    click_in(app, Position::new(button.x + 2, button.y + button.height / 2), area);
}

fn no_bots() -> (App, Receiver<AppEvent>, TempDir) {
    let (mut app, rx) = empty_app();
    let dir = TempDir::new();
    app.set_bots_dir(dir.path());
    app.window_attached();
    (app, rx, dir)
}

fn find(text: &str, needle: &str) -> Position {
    let (y, row) = text.lines().enumerate().find(|(_, row)| row.contains(needle)).expect("the text on screen");
    let x = row[..row.find(needle).expect("found")].chars().count();
    Position::new(u16::try_from(x).expect("x") + 1, u16::try_from(y).expect("y"))
}

fn two_bots() -> Chat {
    let mut c = chat(ANSWER);
    folder::create(c.dir.path(), &bot("digest", "Digest"), "").expect("create");
    c.app.set_bots_dir(c.dir.path());
    c.app.window_attached();
    c
}

#[test]
fn the_button_is_there_with_no_bot_and_opens_the_lobby() {
    let (mut app, _rx, _dir) = no_bots();
    press_button(&mut app, AREA);
    assert!(app.bots_page() && !app.bot_shown());
    assert!(app.visible_tab().is_none());
    let text = drawn_tall(&mut app);
    assert!(text.contains("Meet your bots") && text.contains("Start from an idea"), "{text}");
}

#[test]
fn a_window_that_keeps_no_bots_has_no_button() {
    let (mut app, _rx) = empty_app();
    render(&mut app, AREA);
    assert!(app.layout(AREA).bots_button.is_empty());
}

#[test]
fn the_button_opens_the_first_bot_and_then_the_last_one_shown() {
    let mut c = two_bots();
    press_button(&mut c.app, AREA);
    assert_eq!(c.app.shown_bot(), Some(0));
    c.app.show_bot(1);
    press_back(&mut c.app);
    assert!(!c.app.bots_page());
    press_button(&mut c.app, AREA);
    assert_eq!(c.app.shown_bot(), Some(1));
}

#[test]
fn the_button_counts_the_unread_answers_of_every_bot() {
    let mut c = two_bots();
    c.app.panels.bots.live[0].unread = 2;
    c.app.panels.bots.live[1].unread = 1;
    render(&mut c.app, AREA);
    let button = c.app.layout(AREA).bots_button;
    let text = drawn(&mut c.app);
    let row: String = text.lines().nth(usize::from(button.y)).expect("the title row").chars().collect();
    assert!(row.contains("●3 Bots ›"), "{row}");
}

#[test]
fn the_page_hides_the_agents_section_and_the_projects() {
    let mut c = two_bots();
    c.app.config.agents_section = true;
    press_button(&mut c.app, AREA);
    let a = c.app.layout(AREA);
    assert!(a.agents.is_empty() && a.list.is_empty() && !a.bots_list.is_empty());
    let text = drawn(&mut c.app);
    assert!(!text.contains("no agents running") && text.contains("‹ Projects"), "{text}");
}

#[test]
fn removing_the_shown_bot_leaves_the_lobby() {
    let mut c = two_bots();
    c.app.show_bot(0);
    let id = c.app.panels.bots.live[0].bot.id.clone();
    c.app.bot_remove(&id).expect("removed");
    assert!(c.app.bots_page() && !c.app.bot_shown());
    assert_eq!(c.app.lobby(), Some(ui::Lobby::Pick));
    assert!(drawn(&mut c.app).contains("Pick a bot"));
}

#[test]
fn removing_the_last_bot_leaves_the_lobby_that_says_what_a_bot_is() {
    let mut c = chat(ANSWER);
    c.app.show_bot(0);
    c.app.bot_remove("prs").expect("removed");
    assert_eq!(c.app.lobby(), Some(ui::Lobby::Empty));
}

#[test]
fn new_bot_in_the_lobby_opens_the_form() {
    let (mut app, _rx, _dir) = no_bots();
    press_button(&mut app, TALL);
    let text = drawn_tall(&mut app);
    click_in(&mut app, find(&text, ui::NEW_BOT), TALL);
    assert!(matches!(&app.overlay, Some(Overlay::BotForm(form)) if form.name.is_empty()));
}

#[test]
fn an_idea_opens_the_form_with_its_name_and_what_it_is_for() {
    let (mut app, _rx, _dir) = no_bots();
    press_button(&mut app, TALL);
    let text = drawn_tall(&mut app);
    click_in(&mut app, find(&text, "Releases"), TALL);
    let Some(Overlay::BotForm(form)) = &app.overlay else { panic!("the form is open") };
    assert_eq!((form.name.text(), form.about.text()), (ui::IDEAS[2].0.to_string(), ui::IDEAS[2].1.to_string()));
}

#[test]
fn esc_in_the_lobby_closes_a_panel_and_otherwise_stays() {
    let (mut app, _rx, _dir) = no_bots();
    press_button(&mut app, AREA);
    app.toggle_todo();
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(!app.panel_shown() && app.bots_page());
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.bots_page());
}

#[test]
fn a_move_of_the_keys_menu_leaves_the_page_and_new_bot_keeps_it() {
    let (mut app, _rx, _dir) = no_bots();
    app.config.prefix_key = "ctrl+]".into();
    press_button(&mut app, AREA);
    send_key(&mut app, KeyCode::Char(']'), KeyModifiers::CONTROL);
    send_key(&mut app, KeyCode::Char('b'), KeyModifiers::NONE);
    assert!(matches!(app.overlay, Some(Overlay::BotForm(_))) && app.bots_page());
    app.overlay = None;
    send_key(&mut app, KeyCode::Char(']'), KeyModifiers::CONTROL);
    send_key(&mut app, KeyCode::Char('|'), KeyModifiers::NONE);
    assert!(!app.bots_page());
}

#[test]
fn the_page_is_not_saved() {
    let mut c = chat(ANSWER);
    c.app.show_bot(0);
    let (mut other, _rx) = empty_app();
    other.restore(&c.app.state(), AREA);
    assert!(!other.bots_page());
}

#[cfg(unix)]
#[test]
fn keys_in_the_lobby_never_reach_a_pane() {
    let (mut app, rx, _dir) = no_bots();
    app.open_here(AREA).expect("open the first project");
    type_line(&mut app, "echo cat-\"\"starts; cat -v");
    shows(&mut app, &rx, "cat-starts");
    press_button(&mut app, AREA);
    send_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE);
    app.handle_event(AppEvent::Input(Event::Paste("pasted".into())), AREA).expect("paste");
    press_back(&mut app);
    type_line(&mut app, "markerend");
    shows(&mut app, &rx, "markerend");
    let text = screen(&mut app);
    assert!(!text.contains("pasted") && !text.contains("qmarker"), "{text}");
}

mod compact {
    use super::*;

    #[test]
    fn the_button_opens_the_list_of_bots_and_projects_leaves_the_page() {
        let mut c = chat(ANSWER);
        c.app.window_attached();
        c.app.screen.nav = Some(ui::Nav::Projects);
        press_button(&mut c.app, SMALL);
        assert_eq!((c.app.screen.nav, c.app.bots_page(), c.app.bot_shown()), (Some(ui::Nav::Bots), true, false));
        let back = c.app.layout(SMALL).shown(c.app.screen.nav).back;
        click_in(&mut c.app, Position::new(back.x + 2, back.y + 1), SMALL);
        assert_eq!((c.app.screen.nav, c.app.bots_page()), (Some(ui::Nav::Projects), false));
    }

    #[test]
    fn with_no_bot_the_button_opens_the_lobby() {
        let (mut app, _rx, _dir) = no_bots();
        app.screen.nav = Some(ui::Nav::Projects);
        press_button(&mut app, SMALL);
        assert_eq!((app.screen.nav, app.lobby()), (None, Some(ui::Lobby::Empty)));
        let text = format!("{}", render(&mut app, SMALL).backend());
        assert!(text.contains("Meet your bots"), "{text}");
    }

    #[test]
    fn the_menu_button_of_the_bar_opens_the_list_from_the_lobby() {
        let (mut app, _rx, _dir) = no_bots();
        app.screen.nav = Some(ui::Nav::Projects);
        press_button(&mut app, SMALL);
        let bar = app.layout(SMALL).bar;
        click_in(&mut app, bar.as_position(), SMALL);
        assert_eq!(app.screen.nav, Some(ui::Nav::Bots));
        send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!((app.screen.nav, app.bots_page()), (None, true));
    }
}
