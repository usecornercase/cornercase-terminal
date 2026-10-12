use super::*;

fn row(section: &'static str, label: &str, value: &str, note: &str) -> SettingsRow {
    SettingsRow {
        section,
        label: label.into(),
        value: value.into(),
        note: note.into(),
        dangerous: false,
        removable: false,
        movable: false,
    }
}

const TABS: [&str; 4] = ["Worktrees", "Agents", "Issues", "TUI"];

fn source(label: &str, note: &str) -> SettingsRow {
    SettingsRow { movable: true, ..row("Sources shown", label, "", note) }
}

fn settings(cursor: usize) -> Settings {
    Settings {
        tabs: TABS.to_vec(),
        tab: 2,
        rows: vec![
            SettingsRow { removable: true, ..row("Accounts", "Shortcut API token", "@ana in acme", "saved") },
            row("Accounts", "Linear API key", "not connected", "enter pastes one"),
            source("[x] All", "every source together"),
            source("[x] GitHub", ""),
            source("[ ] Linear", ""),
        ],
        cursor,
        edit: None,
        pick: None,
        note: None,
        hint: "enter changes the selected setting".into(),
        submit: "done",
    }
}

fn with(settings: Settings) -> View<'static> {
    View { overlay: Some(Overlay::Settings(settings)), ..view(&["shop"]) }
}

#[test]
fn are_drawn_where_their_area_says() {
    assert_eq!(corners_of_its_area(&with(settings(0))), (String::from("╭"), String::from("╯")));
}

fn sections() -> Vec<&'static str> {
    settings(0).sections()
}

fn layout<'a>(sections: &'a [&'a str]) -> SettingsLayout<'a> {
    SettingsLayout {
        sections,
        tabs: &TABS,
        removable: &[true, false, false, false, false],
        movable: &[false, false, true, true, true],
        cursor: 0,
        pick: None,
    }
}

#[test]
fn renders_the_sections_and_rows() {
    insta::assert_snapshot!(render(&with(settings(1))).backend());
}

#[test]
fn renders_a_pick() {
    let pick = SettingsPick {
        title: "How should claude start?".into(),
        filter: "pl".into(),
        items: vec![
            ("plan".into(), "--permission-mode plan".into(), false),
            ("skip permissions (dangerous)".into(), "--dangerously-skip-permissions".into(), true),
        ],
        selected: Some(0),
        scroll: 0,
    };
    let agents = Settings { tab: 1, rows: vec![row("Agent", "default agent", "claude", "")], ..settings(0) };
    insta::assert_snapshot!(render(&with(Settings { pick: Some(pick), ..agents })).backend());
}

#[test]
fn headers_go_before_the_first_row_of_each_section() {
    assert_eq!(
        settings_lines(&["Folder", "Accounts", "Accounts"]),
        [
            SettingsLine::Header(0),
            SettingsLine::Row(0),
            SettingsLine::Blank,
            SettingsLine::Header(1),
            SettingsLine::Row(1),
            SettingsLine::Row(2)
        ]
    );
}

#[test]
fn a_row_without_a_section_has_no_header() {
    assert_eq!(
        settings_lines(&["", "Agent"]),
        [SettingsLine::Row(0), SettingsLine::Blank, SettingsLine::Header(1), SettingsLine::Row(1)]
    );
}

#[test]
fn the_active_tab_is_filled() {
    let t = render(&with(settings(0)));
    let issues = settings_tabs(settings_area(AREA), &TABS)[2];
    assert_eq!(t.backend().buffer()[issues.as_position()].bg, DARK_SURFACE);
}

#[test]
fn scrolls_to_keep_the_selected_row_visible() {
    let sections = vec!["A"; 40];
    let at = settings_scroll(&sections, 30, 10);
    let line = settings_lines(&sections).iter().position(|l| *l == SettingsLine::Row(30)).expect("row");
    assert!(line >= at && line < at + 10, "line {line} scroll {at}");
}

#[test]
fn a_dangerous_value_is_red() {
    let claude = SettingsRow {
        dangerous: true,
        ..row("How each agent starts", "claude", "skip permissions (dangerous)", "the default agent")
    };
    let r = settings_row(AREA, &["How each agent starts"], 0, 0);
    let t = render(&with(Settings { tab: 1, rows: vec![claude], ..settings(0) }));
    let cell = (r.x..r.right()).map(|x| &t.backend().buffer()[(x, r.y)]).find(|c| c.symbol() == "k").expect("text");
    assert_eq!(cell.fg, Color::Red);
}

#[test]
fn the_edit_line_puts_the_cursor_after_the_value() {
    let edit = SettingsEdit { label: "Linear API key".into(), value: "•••".into() };
    let mut t = render(&with(Settings { edit: Some(edit), ..settings(2) }));
    let row = settings_edit(settings_area(AREA));
    let x =
        row.x + u16::try_from(INPUT_PROMPT.chars().count() + "Linear API key: ".chars().count() + 3).expect("width");
    assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(x, row.y));
}

#[rstest]
#[case::a_tab(settings_tabs(settings_area(AREA), &TABS)[1].as_position(), Some(SettingsHit::Tab(1)))]
#[case::a_row(settings_row(AREA, &sections(), 0, 1).as_position(), Some(SettingsHit::Row(1)))]
#[case::remove(settings_remove(settings_row(AREA, &sections(), 0, 0)).as_position(), Some(SettingsHit::Remove(0)))]
#[case::move_up(settings_moves(settings_row(AREA, &sections(), 0, 3))[0].as_position(), Some(SettingsHit::MoveUp(3)))]
#[case::move_down(settings_moves(settings_row(AREA, &sections(), 0, 3))[1].as_position(), Some(SettingsHit::MoveDown(3)))]
#[case::done(settings_done(settings_area(AREA)).as_position(), Some(SettingsHit::Done))]
#[case::a_header(settings_body(settings_area(AREA)).as_position(), None)]
fn maps_clicks(#[case] pos: Position, #[case] expected: Option<SettingsHit>) {
    let sections = sections();
    assert_eq!(settings_hit(AREA, &layout(&sections), pos), expected);
}

#[test]
fn a_click_in_a_pick_chooses_that_item() {
    let sections = sections();
    let list = settings_pick_list(settings_area(AREA));
    let pos = Position::new(list.x + 2, list.y + 1);
    let pick = SettingsLayout { pick: Some((3, 0)), ..layout(&sections) };
    assert_eq!(settings_hit(AREA, &pick, pos), Some(SettingsHit::Pick(1)));
}
