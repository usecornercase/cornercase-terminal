use super::*;
use crate::editor::Editor;

fn row(section: &'static str, label: &str, value: &str, note: &str) -> SettingsRow {
    SettingsRow {
        section,
        label: label.into(),
        value: value.into(),
        note: note.into(),
        dangerous: false,
        muted: false,
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
        tabs: TABS.map(String::from).to_vec(),
        dimmed: Vec::new(),
        tab: 2,
        rows: vec![
            SettingsRow { removable: true, ..row("Accounts", "Shortcut API token", "@ana in acme", "saved") },
            row("Accounts", "Linear API key", "not connected", "enter pastes one"),
            source("[x] All", "every source together"),
            source("[x] GitHub", ""),
            source("[ ] Linear", ""),
        ],
        cursor,
        find: SettingsFind { field: Editor::default(), focused: true, searching: false },
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
fn renders_the_forum_mode_and_closed_workspaces_under_the_discord_phone() {
    let remote = Settings {
        tabs: ["Worktrees", "Agents", "Remote"].map(String::from).to_vec(),
        tab: 2,
        rows: vec![
            row("Browser", "remote access", "[ ] off", ""),
            row("Discord", "bot", "@cornercase_bot", ""),
            SettingsRow {
                removable: true,
                ..row("Discord", "phone", "@ada", "agents message it while you are away · enter pairs another")
            },
            row("Discord", "forum mode", "[x] on", "add the app to your server"),
            row("Discord", "closed workspaces", "archive", "what happens to the post of a workspace you close"),
        ],
        ..settings(3)
    };
    insta::assert_snapshot!(render(&with(remote)).backend());
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
#[case::share(settings_share(settings_area(AREA)).as_position(), Some(SettingsHit::Share))]
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

fn searching(query: &str, rows: Vec<SettingsRow>) -> Settings {
    let mut field = Editor::default();
    field.insert(query);
    let counts = ["Worktrees 0", "Agents 2", "Issues 0", "TUI 1"];
    Settings {
        tabs: counts.map(String::from).to_vec(),
        dimmed: vec![true, false, true, false],
        tab: 1,
        rows,
        cursor: 1,
        find: SettingsFind { field, focused: true, searching: true },
        hint: "↑↓ picks a result · enter goes to it · esc clears the search".into(),
        ..settings(0)
    }
}

fn results() -> Vec<SettingsRow> {
    let hidden = row("", "Remote › closed workspaces", "", "shows once forum mode is on");
    vec![
        row("", "Agents › default agent", "claude", ""),
        row("", "Remote › Telegram › bot", "not set", "paste its token"),
        SettingsRow { muted: true, ..hidden },
    ]
}

#[test]
fn renders_the_results_of_a_search() {
    insta::assert_snapshot!(render(&with(searching("def", results()))).backend());
}

#[test]
fn renders_a_search_that_finds_nothing() {
    insta::assert_snapshot!(render(&with(searching("zebra", Vec::new()))).backend());
}

#[test]
fn renders_a_search_in_a_narrow_window() {
    insta::assert_snapshot!(render_sized(&with(searching("def", results())), 60, 20).backend());
}

#[test]
fn renders_a_search_in_a_compact_window() {
    insta::assert_snapshot!(render_sized(&with(searching("def", results())), 44, 24).backend());
}

#[test]
fn dims_a_tab_with_no_results() {
    let shown = with(searching("def", results()));
    let area = settings_area(AREA);
    let names = ["Worktrees 0", "Agents 2", "Issues 0", "TUI 1"];
    let rects = settings_tabs(area, &names);
    let backend = render(&shown);
    let dim = |i: usize| backend.backend().buffer()[rects[i].as_position()].modifier.contains(Modifier::DIM);
    assert_eq!((dim(0), dim(1)), (true, false));
}

#[test]
fn a_click_on_the_field_is_a_hit_of_the_field() {
    let area = Rect::new(0, 0, 100, 20);
    let at = settings_find(settings_area(area)).as_position();
    assert_eq!(settings_hit(area, &layout(&sections()), at), Some(SettingsHit::Find));
}
