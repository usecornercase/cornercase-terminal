use libghostty_vt::style::RgbColor;
use serde_json::{Value, json};

use super::*;
use crate::control::theme::{ThemeFrom, ThemeInfo};
use crate::host::theme::HostTheme;
use crate::test_util::theme;

const DARK: RgbColor = RgbColor { r: 26, g: 27, b: 38 };
const LIGHT: RgbColor = RgbColor { r: 251, g: 250, b: 246 };

fn give(app: &mut App, colours: &HostTheme) -> Value {
    ok(app, "theme", &json!({"theme": colours}))
}

fn clear(app: &mut App) -> Value {
    ok(app, "theme", &json!({"clear": true}))
}

fn backgrounds(app: &App) -> Vec<Option<RgbColor>> {
    app.backgrounds()
}

fn probe(app: &mut App, rx: &Receiver<AppEvent>, pane: u64, marker: u32) {
    let line = format!(
        "clear; stty -icanon -echo min 0 time 20; printf '\\033]11;?\\007'; cat -v; stty sane; echo; echo asked-\"\"{marker}"
    );
    ok(app, "input", &json!({"pane": pane, "events": [{"text": line}, {"key": {"code": "enter"}}]}));
    let done = format!("asked-{marker}");
    pump_until(app, rx, "the probe ends", |a| pane_screen(a, pane).lines().any(|l| l == done));
}

#[test]
fn a_theme_colours_every_open_terminal_and_the_ones_opened_after() {
    let (mut app, _rx) = app();
    let colours = theme(DARK);
    assert_eq!(backgrounds(&app), [None]);

    give(&mut app, &colours);
    assert_eq!(backgrounds(&app), [Some(DARK)]);

    done(now(&mut app, None, new_tab(None)));
    assert_eq!(backgrounds(&app), [Some(DARK), Some(DARK)]);
}

#[test]
fn a_pane_that_asks_gets_the_given_background_before_and_after_a_change() {
    let (mut app, rx) = app();
    let id = first(&app);

    give(&mut app, &theme(DARK));
    probe(&mut app, &rx, id, 1);
    assert!(pane_screen(&app, id).contains("rgb:1a1a/1b1b/2626"), "{}", pane_screen(&app, id));

    give(&mut app, &theme(LIGHT));
    probe(&mut app, &rx, id, 2);
    assert!(pane_screen(&app, id).contains("rgb:fbfb/fafa/f6f6"), "{}", pane_screen(&app, id));
}

#[test]
fn reading_says_where_the_colours_come_from() {
    let (mut app, _rx) = app();
    assert_eq!(ok(&mut app, "theme", &json!({})), json!({"from": "default", "scheme": null}));

    assert_eq!(give(&mut app, &theme(DARK)), json!({"from": "client", "scheme": "dark"}));
    assert_eq!(ok(&mut app, "theme", &json!({})), json!({"from": "client", "scheme": "dark"}));

    assert_eq!(clear(&mut app), json!({"from": "default", "scheme": null}));
    assert_eq!(backgrounds(&app), [None]);
}

#[test]
fn the_report_carries_where_the_colours_come_from() {
    let (mut app, _rx) = app();
    give(&mut app, &theme(LIGHT));

    let report = ok(&mut app, "status", &json!({}));

    assert_eq!(report["theme"], json!({"from": "client", "scheme": "light"}));
}

#[test]
fn a_theme_that_is_not_whole_is_malformed_and_changes_nothing() {
    let (mut app, _rx) = app();
    give(&mut app, &theme(DARK));
    let mut no_background = serde_json::to_value(theme(LIGHT)).expect("json");
    no_background["background"] = Value::Null;
    let mut no_ninth = serde_json::to_value(theme(LIGHT)).expect("json");
    no_ninth["palette"][9] = Value::Null;

    for bad in [no_background, no_ninth] {
        let refusal = refused(&mut app, "theme", &json!({"theme": bad}));
        assert_eq!(refusal.code, "malformed", "{}", refusal.message);
    }

    assert_eq!(backgrounds(&app), [Some(DARK)]);
}

#[test]
fn colours_together_with_clear_are_malformed() {
    let (mut app, _rx) = app();

    let refusal = refused(&mut app, "theme", &json!({"theme": theme(DARK), "clear": true}));

    assert_eq!(refusal.code, "malformed");
    assert_eq!(backgrounds(&app), [None]);
}

#[test]
fn an_unknown_key_inside_the_theme_is_refused() {
    let (mut app, _rx) = app();
    let mut colours = serde_json::to_value(theme(DARK)).expect("json");
    colours["cursor"] = json!([1, 2, 3]);

    let refusal = refused(&mut app, "theme", &json!({"theme": colours}));

    assert_eq!(refusal.code, "unknown-field");
    assert!(refusal.message.contains("args.theme.cursor"), "{}", refusal.message);
    assert_eq!(backgrounds(&app), [None]);
}

#[test]
fn a_view_is_refused() {
    let (mut app, _rx) = app();
    let text = json!({"contract": 1, "view": 3, "command": "theme", "args": {}}).to_string();

    app.request(CLIENT, &text, Some(AREA), Instant::now());

    let answers = app.take_answers();
    assert!(answers[0].1.contains("takes no view"), "{answers:?}");
}

#[test]
fn a_window_after_a_client_theme_draws_with_its_own_colours_but_leaves_the_terminals_alone() {
    let (mut app, _rx) = app();
    give(&mut app, &theme(DARK));

    let painted = app.retheme(theme(LIGHT));

    assert!(!painted);
    assert_eq!(backgrounds(&app), [Some(DARK)]);
    assert_eq!(app.theme.background, Some(LIGHT));
    assert_eq!(app.theme_info(), ThemeInfo { from: ThemeFrom::Client, scheme: app.theme_info().scheme });
}

#[test]
fn clearing_after_a_window_gives_the_terminals_its_colours() {
    let (mut app, _rx) = app();
    give(&mut app, &theme(DARK));
    app.retheme(theme(LIGHT));

    let info = clear(&mut app);

    assert_eq!(info["from"], "window");
    assert_eq!(backgrounds(&app), [Some(LIGHT)]);
}
