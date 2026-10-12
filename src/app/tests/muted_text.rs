use libghostty_vt::style::RgbColor;
use ratatui::style::Color;

use super::*;

#[test]
fn a_terminal_that_keeps_its_palette_to_itself_gets_a_grey_that_shows() {
    let (mut app, _rx) = empty_app();
    app.set_theme(HostTheme {
        background: Some(RgbColor { r: 0x1d, g: 0x20, b: 0x22 }),
        ..HostTheme::default()
    });
    let t = render(&mut app, AREA);

    let settings = areas().settings;
    assert_eq!(t.backend().buffer()[(settings.x + 2, settings.y)].fg, Color::Indexed(243));
}
