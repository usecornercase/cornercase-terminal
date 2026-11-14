use super::*;

const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

fn drawn(app: &mut App) -> String {
    format!("{}", render(app, SMALL).backend())
}

#[test]
fn the_projects_menu_lists_bots_beside_the_agents_and_a_tap_shows_the_chat() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    c.app.screen.nav = Some(ui::Nav::Projects);
    let button = c.app.layout(SMALL).shown(c.app.screen.nav).bots_button;
    assert!(!button.is_empty());
    click_in(&mut c.app, button.as_position(), SMALL);
    assert_eq!(c.app.screen.nav, Some(ui::Nav::Bots));
    assert!(drawn(&mut c.app).contains("PRs"));
    let a = c.app.layout(SMALL).shown(c.app.screen.nav);
    let row = ui::bot_rows(a.bots_list, a.pitch, 1, 0).item(0);
    assert_eq!(row.height, ui::COMPACT_PITCH);
    click_in(&mut c.app, Position::new(row.x + 3, row.y + 1), SMALL);
    assert_eq!((c.app.shown_bot(), c.app.screen.nav), (Some(0), None));
    let text = drawn(&mut c.app);
    assert!(text.contains("Bots › PRs") && text.contains("Message PRs…"), "{text}");
}

#[test]
fn the_menu_button_of_the_bar_opens_the_bots_while_a_chat_shows() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    c.app.show_bot(0);
    let bar = c.app.layout(SMALL).bar;
    click_in(&mut c.app, bar.as_position(), SMALL);
    assert_eq!(c.app.screen.nav, Some(ui::Nav::Bots));
}
