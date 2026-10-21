use super::*;
use crate::control::config::{SetupPreview, change, section};
use crate::settings::share::Shared;
use crate::settings::share::dialog::tests::previewed;
use crate::settings::share::dialog::{BACK, Dialog, IMPORT};

const WIDE: Rect = Rect { x: 0, y: 0, width: 144, height: 42 };

fn paste_page(text: &str) -> ShareSettings {
    ShareSettings::of(&Dialog::new(text), WIDE)
}

fn import_page(untick: &[usize]) -> ShareSettings {
    let mut dialog = previewed();
    for section in untick {
        dialog.toggle(*section);
    }
    ShareSettings::of(&dialog, WIDE)
}

fn with(share: ShareSettings) -> View<'static> {
    View { overlay: Some(Overlay::ShareSettings(Box::new(share))), ..view(&["shop"]) }
}

fn cell_of(t: &Terminal<TestBackend>, r: Rect) -> &ratatui::buffer::Cell {
    &t.backend().buffer()[(r.x + 1, r.y)]
}

#[test]
fn is_drawn_where_its_area_says() {
    assert_eq!(corners_of_its_area(&with(paste_page(""))), (String::from("╭"), String::from("╯")));
}

#[test]
fn renders_the_paste_page() {
    let t = render_sized(&with(paste_page("version = 1\n[interface]\nsidebar = \"tree\"")), WIDE.width, WIDE.height);
    insta::assert_snapshot!(t.backend());
}

#[test]
fn renders_the_import_page() {
    let t = render_sized(&with(import_page(&[])), WIDE.width, WIDE.height);
    insta::assert_snapshot!(t.backend());
}

#[test]
fn draws_import_dimmed_with_nothing_ticked() {
    let view = with(import_page(&[0, 2]));
    let t = render_sized(&view, WIDE.width, WIDE.height);
    let import = share_settings_buttons(WIDE, &[BACK, IMPORT])[1];
    assert_eq!(cell_of(&t, import).fg, Color::DarkGray);
}

#[test]
fn draws_risks_in_red() {
    let t = render_sized(&with(import_page(&[])), WIDE.width, WIDE.height);
    let list = share_settings_list(WIDE);
    assert_eq!(cell_of(&t, Rect { y: list.y + 3, ..list }).fg, Color::Red);
}

#[test]
fn draws_each_line_of_a_setup_on_its_own_row_in_red() {
    let mut dialog = Dialog::new("version = 1");
    let changes = vec![change("repositories.setup", "~/code/web-shop", "nothing", "npm ci\ncp .env.example .env")];
    let preview = SetupPreview { sections: vec![section("repositories", changes, Vec::new())], notes: Vec::new() };
    dialog.previewed(Ok((Shared::default(), preview)));
    let t = render_sized(&with(ShareSettings::of(&dialog, WIDE)), WIDE.width, WIDE.height);
    let list = share_settings_list(WIDE);
    let buffer = t.backend().buffer();
    let row = |y: u16| (list.x..list.right()).map(|x| buffer[(x, list.y + y)].symbol()).collect::<String>();
    let script = |y: u16| (row(y).trim_end().to_string(), buffer[(list.x + 10, list.y + y)].fg);
    assert_eq!(
        (script(2), script(3)),
        (("          npm ci".to_string(), Color::Red), ("          cp .env.example .env".to_string(), Color::Red))
    );
}

#[test]
fn maps_a_click_on_a_section_and_on_a_button() {
    let view = import_page(&[]);
    let list = share_settings_list(WIDE);
    let button = share_settings_buttons(WIDE, &view.buttons)[1];
    assert_eq!(
        (
            share_settings_hit(WIDE, &view, list.as_position()),
            share_settings_hit(WIDE, &view, Position::new(list.x, list.y + 1)),
            share_settings_hit(WIDE, &view, Position::new(list.x, list.y + 5)),
            share_settings_hit(WIDE, &view, button.as_position()),
        ),
        (Some(ShareHit::Line(0)), Some(ShareHit::Line(1)), Some(ShareHit::Line(5)), Some(ShareHit::Button(1)))
    );
}

#[test]
fn maps_a_click_in_the_field_to_its_place() {
    let text = share_settings_text(WIDE);
    let pos = Position::new(text.x + 3, text.y + 1);
    assert_eq!(share_settings_hit(WIDE, &paste_page("a\nbcdef"), pos), Some(ShareHit::Text { line: 1, col: 3 }));
}
