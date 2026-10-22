use ratatui::buffer::Buffer;
use rstest::rstest;

use super::*;
use crate::graphics::Support;

const FOLDER: &str = "\u{e5ff}";
const FILE: &str = "\u{e66a}";
const FILES_BUTTON: &str = "\u{ea7b}";

fn with_tree(icons: &str, icon_bar: bool) -> (TempDir, App, Receiver<AppEvent>) {
    let repo = git_repo(&[("README.md", "# shop\n"), ("src/main.rs", "fn main() {}\n")]);
    let (mut app, rx) = app_in(repo.path(), no_config());
    let pos = app.layout(AREA).files_button.as_position();
    click(&mut app, pos);
    let sight = Sight::default();
    wait_until("the tree is listed", || {
        app.refresh(Instant::now());
        drain(&mut app, &rx);
        text(&render_frame(&mut app, AREA, &sight).0).contains("README.md")
    });
    app.config.icons = icons.into();
    app.config.icon_bar = icon_bar;
    (repo, app, rx)
}

fn text(buffer: &Buffer) -> String {
    buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect()
}

fn sight(nerd_font: bool) -> Sight {
    Sight { support: Support { nerd_font, ..Support::default() }, ..Sight::default() }
}

fn drawn(app: &mut App, nerd_font: bool) -> String {
    text(&render_frame(app, AREA, &sight(nerd_font)).0)
}

#[test]
fn auto_follows_each_windows_terminal() {
    let (_repo, mut app, _rx) = with_tree("auto", false);

    let (ghostty, plain) = (drawn(&mut app, true), drawn(&mut app, false));

    assert_eq!(
        (ghostty.contains(FOLDER), ghostty.contains(FILE), plain.contains(FOLDER), plain.contains(FILE)),
        (true, true, false, false)
    );
}

#[rstest]
#[case::on_without_the_icon_bar("on", false, false, true)]
#[case::on_in_a_plain_terminal("on", true, false, true)]
#[case::off_with_the_symbols("off", false, true, false)]
#[case::off_even_with_the_icon_bar("off", true, true, false)]
#[case::auto_with_the_icon_bar("auto", true, false, true)]
#[case::auto_in_a_plain_terminal("auto", false, false, false)]
#[case::an_unknown_value_is_auto("often", false, true, true)]
fn the_setting_decides(#[case] icons: &str, #[case] icon_bar: bool, #[case] nerd_font: bool, #[case] expected: bool) {
    let (_repo, mut app, _rx) = with_tree(icons, icon_bar);

    assert_eq!(drawn(&mut app, nerd_font).contains(FOLDER), expected);
}

#[test]
fn the_icon_bar_turns_the_panel_glyphs_on_under_auto() {
    let (_repo, mut app, _rx) = with_tree("auto", true);
    let seen = drawn(&mut app, false);
    assert_eq!((seen.contains(FILE), seen.contains(FILES_BUTTON)), (true, true));
}
