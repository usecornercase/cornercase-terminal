use super::*;

fn with_bar() -> View<'static> {
    let details =
        Details { model: Some("Opus 5.5".into()), percent: Some(15), subagents: 0, memory: None, account: None };
    let claude = TabEntry { status: Some(Status::Working), details: details.clone(), ..TabEntry::from("claude") };
    let tabs = vec![claude, TabEntry::from("zsh")];
    View {
        has_project: true,
        workspaces: vec![WorkspaceEntry {
            collapsed: false,
            name: "main".into(),
            tabs: tabs.clone(),
            behind: 0,
            pull: None,
            removing: false,
            setting_up: false,
            closed: 0,
        }],
        active_tab: Some(0),
        tab_bar: Some(tab_bar::TabBar { tabs, active: Some(0), details, scroll: 0 }),
        ..view(&["cornercase"])
    }
}

fn bar() -> Rect {
    layout(AREA, Widths::default()).with_tab_bar(true).tab_bar
}

#[test]
fn renders_the_tabs_above_the_pane_and_none_in_the_list() {
    insta::assert_snapshot!(render(&with_bar()).backend());
}

#[test]
fn the_details_row_is_muted() {
    let t = render(&with_bar());
    let r = bar();
    assert_eq!(row_text(&t, Rect { y: r.y + 1, height: 1, ..r }).trim(), "Opus 5.5 · 15%");
    assert_eq!(t.backend().buffer()[(r.x + 1, r.y + 1)].fg, Color::DarkGray);
}

#[test]
fn the_active_tab_is_on_the_surface() {
    let t = render(&with_bar());
    let first = with_bar().tab_bar.expect("a bar").strip(bar()).item(0);
    assert_eq!(t.backend().buffer()[(first.x, first.y)].bg, DARK_SURFACE);
}

#[test]
fn the_menu_and_close_buttons_show_only_on_hover() {
    let v = with_bar();
    let strip = v.tab_bar.as_ref().expect("a bar").strip(bar());
    let (menu, close) = (strip.menu(1), strip.close(1));
    let symbols = |v: &View| {
        let t = render(v);
        let cell = |x: u16, y: u16| t.backend().buffer()[(x, y)].symbol().to_string();
        (cell(menu.right() - 1, menu.y), cell(close.x + 1, close.y))
    };
    assert_eq!(symbols(&v), (" ".into(), " ".into()));
    let hovered = View { hover: Some(close.as_position()), ..with_bar() };
    assert_eq!(symbols(&hovered), (ROW_MENU_ICON.into(), "×".into()));
}

#[test]
fn a_split_tab_shows_how_many_other_panes_it_has() {
    let mut v = with_bar();
    v.tab_bar.as_mut().expect("a bar").tabs[1].others = 2;
    let t = render(&v);
    let item = v.tab_bar.as_ref().expect("a bar").strip(bar()).item(1);
    assert_eq!(row_text(&t, item).trim_end(), " › zsh +2");
    let count = t.backend().buffer()[(item.x + 7, item.y)].clone();
    assert_eq!((count.symbol(), count.fg), ("+", Color::DarkGray));
}

#[test]
fn a_landing_scrolled_out_of_the_bar_draws_nothing() {
    let tabs: Vec<TabEntry> = (0..12).map(|i| TabEntry::from(format!("tab {i}"))).collect();
    let mut v = with_bar();
    v.tab_bar = Some(tab_bar::TabBar { tabs, active: Some(11), details: Details::default(), scroll: 6 });
    v.drag = Some(Drag::Bar(11, Some(0)));
    let t = render(&v);
    assert_ne!(t.backend().buffer()[(0, 0)].symbol(), "│");
}

#[test]
fn without_a_project_there_is_no_bar() {
    let v = View { has_project: false, ..with_bar() };
    let t = render(&v);
    assert_eq!(row_text(&t, bar()).trim(), "");
}

#[test]
fn the_details_row_names_the_account() {
    let mut v = with_bar();
    v.tab_bar.as_mut().expect("a bar").details.account = Some("work".into());
    let t = render(&v);
    let r = bar();
    assert_eq!(row_text(&t, Rect { y: r.y + 1, height: 1, ..r }).trim(), "Opus 5.5 · work · 15%");
    insta::assert_snapshot!(t.backend());
}
