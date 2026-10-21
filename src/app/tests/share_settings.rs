use super::*;
use crate::settings::share::HEADER;
use crate::settings::share::dialog::{BACK, COPY, Dialog, IMPORT, PREVIEW, Page as SharePage};
use crate::settings::{Edit, Row};

const SETUP: &str = "version = 1\n[agents]\nprompt = \"Fix {url}\"\n[interface]\nsidebar = \"tree\"\n";

struct Setup {
    app: App,
    _config: TempDir,
    _dir: TempDir,
}

fn settings() -> Setup {
    let (dir, config) = (TempDir::new(), TempDir::new());
    let (mut app, _rx) = app_in(dir.path(), config.path().join("config.json"));
    app.issues.clear_env_tokens();
    click(&mut app, areas().settings.as_position());
    Setup { app, _config: config, _dir: dir }
}

fn share() -> Setup {
    let mut s = settings();
    click(&mut s.app, ui::settings_share(ui::settings_area(AREA)).as_position());
    s
}

fn dialog(app: &App) -> &Dialog {
    let Some(Overlay::ShareSettings(dialog)) = &app.overlay else { panic!("share is not open") };
    dialog
}

fn press(app: &mut App, label: &str) {
    let buttons = dialog(app).buttons();
    let i = buttons.iter().position(|b| *b == label).expect("the button is there");
    click(app, ui::share_settings_buttons(AREA, &buttons)[i].as_position());
}

fn paste(app: &mut App, text: &str) {
    app.handle_event(AppEvent::Input(Event::Paste(text.into())), AREA).expect("handle paste");
}

fn shown(app: &mut App) -> String {
    let t = render(app, AREA);
    let buffer = t.backend().buffer();
    (0..AREA.height)
        .map(|y| (0..AREA.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn click_section(app: &mut App, id: &str) {
    let list = ui::share_settings_list(AREA);
    let lines = dialog(app).lines(usize::from(list.width));
    let line = lines.iter().position(|line| line.text.contains(id) && line.section.is_some()).expect("its line");
    click(app, Position::new(list.x, list.y + u16::try_from(line).expect("a line")));
}

#[test]
fn share_opens_from_the_settings_buttons() {
    let mut s = share();
    assert_eq!(s.app.overlay.as_ref().map(Overlay::name), Some("share settings"));
    assert!(shown(&mut s.app).contains(" copy my setup "));
}

#[test]
fn back_returns_to_the_settings_page_it_came_from() {
    let mut s = settings();
    click(&mut s.app, ui::settings_tabs(ui::settings_area(AREA), &Page::ALL.map(Page::name))[1].as_position());
    click(&mut s.app, ui::settings_share(ui::settings_area(AREA)).as_position());
    press(&mut s.app, BACK);
    let Some(Overlay::Settings(settings)) = &s.app.overlay else { panic!("the settings are not open") };
    assert_eq!(settings.page, Page::ALL[1]);
}

#[test]
fn copy_puts_the_setup_on_the_clipboard() {
    let mut s = share();
    let text = s.app.export_setup().expect("export").text;
    press(&mut s.app, COPY);
    assert!(text.starts_with(HEADER), "{text}");
    assert_eq!(s.app.take_host_writes(), vec![clipboard::osc52(&text)]);
    assert!(toast(&s.app).is_some_and(|t| t.starts_with("setup copied: ")), "{:?}", toast(&s.app));
}

#[test]
fn a_paste_on_a_settings_tab_opens_share_with_the_text() {
    let mut s = settings();
    paste(&mut s.app, SETUP);
    let SharePage::Paste { editor, .. } = &dialog(&s.app).page else { panic!("not on the paste page") };
    assert_eq!(editor.text(), SETUP);
}

#[test]
fn a_paste_while_a_field_is_edited_goes_into_the_field() {
    let mut s = settings();
    if let Some(Overlay::Settings(settings)) = &mut s.app.overlay {
        settings.edit = Some(Edit { row: Row::Folder, input: String::new(), error: None });
    }
    paste(&mut s.app, "/srv/trees");
    let Some(Overlay::Settings(settings)) = &s.app.overlay else { panic!("the settings are not open") };
    assert_eq!(settings.edit.as_ref().map(|edit| edit.input.as_str()), Some("/srv/trees"));
}

#[test]
fn preview_then_import_applies_the_ticked_sections() {
    let mut s = share();
    paste(&mut s.app, SETUP);
    press(&mut s.app, PREVIEW);
    click_section(&mut s.app, "agents");
    press(&mut s.app, IMPORT);
    assert_eq!((s.app.config.sidebar.as_str(), s.app.config.prompt.as_str()), ("tree", "{url}"));
    assert_eq!(toast(&s.app), Some("imported 1 settings and 0 repositories"));
    assert_eq!(s.app.overlay.as_ref().map(Overlay::name), Some("settings"));
}

#[test]
fn an_invalid_text_stays_on_the_page_with_its_reason() {
    let mut s = share();
    paste(&mut s.app, "version = 2\n");
    press(&mut s.app, PREVIEW);
    let SharePage::Paste { error, .. } = &dialog(&s.app).page else { panic!("not on the paste page") };
    let error = error.clone().expect("a reason");
    assert!(error.starts_with("made by a newer cornercase"), "{error}");
    assert!(shown(&mut s.app).contains("made by a newer cornercase"));
}

#[test]
fn esc_goes_back_a_page_at_a_time() {
    let mut s = share();
    paste(&mut s.app, SETUP);
    press(&mut s.app, PREVIEW);
    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(dialog(&s.app).page, SharePage::Paste { .. }));
    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(s.app.overlay.as_ref().map(Overlay::name), Some("settings"));
}

#[test]
fn the_flows_tab_offers_share_too() {
    let mut s = settings();
    let flows = Page::ALL.iter().position(|page| *page == Page::Flows).expect("the flows tab");
    click(&mut s.app, ui::settings_tabs(ui::settings_area(AREA), &Page::ALL.map(Page::name))[flows].as_position());
    let Some(Overlay::Flows { dialog, .. }) = &s.app.overlay else { panic!("the flows tab is not open") };
    assert!(dialog.buttons(crate::flows::dialog::Room::default()).contains(&crate::settings::SHARE));
}
