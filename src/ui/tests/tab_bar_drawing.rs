use super::*;

fn with_bar() -> View<'static> {
    let details = Details {
        model: Some("Opus 5.5".into()),
        percent: Some(15),
        subagents: 0,
        memory: None,
        account: None,
        compacted: false,
        by: None,
    };
    let claude = TabEntry { status: Some(Status::Working), details: details.clone(), ..TabEntry::from("claude") };
    let tabs = vec![claude, TabEntry::from("zsh")];
    View {
        has_project: true,
        workspaces: vec![WorkspaceEntry {
            shared: 0,
            collapsed: false,
            name: "main".into(),
            tabs: tabs.clone(),
            behind: 0,
            pull: None,
            removing: false,
            setting_up: false,
            closed: 0,
            changed: None,
            machine: None,
        }],
        active_tab: Some(0),
        tab_bar: Some(tab_bar::TabBar { tabs, active: Some(0), details, scroll: 0, machine: None }),
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
    v.tab_bar = Some(tab_bar::TabBar { tabs, active: Some(11), details: Details::default(), scroll: 6, machine: None });
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

fn on_a_machine(dim: bool, seen: Option<&str>) -> View<'static> {
    let mut v = with_bar();
    let tag = MachineTag { icon: "☁".into(), name: "vps".into(), dim, seen: seen.map(Into::into) };
    v.tab_bar.as_mut().expect("a bar").machine = Some(tag);
    v
}

#[test]
fn a_closing_tab_reads_closing_and_has_no_buttons() {
    let mut v = with_bar();
    let shown = v.tab_bar.as_mut().expect("a bar");
    shown.tabs[0].closing = true;
    shown.tabs[0].others = 2;
    let item = v.tab_bar.as_ref().expect("a bar").strip(bar()).item(0);
    let hovered = View { hover: Some(item.as_position()), ..v };
    let t = render(&hovered);
    let text = row_text(&t, item);
    assert!(text.trim_end().ends_with("closing…"), "{text:?}");
    assert!(!text.contains("+2") && !text.contains('×') && !text.contains(ROW_MENU_ICON), "{text:?}");
    let name = &t.backend().buffer()[(item.x + 3, item.y)];
    assert_eq!((name.symbol(), name.fg), ("c", Color::DarkGray));
    assert!(!name.modifier.contains(Modifier::BOLD));
}

#[rstest]
#[case::not_linked(true)]
#[case::linked(false)]
fn a_machine_that_is_not_linked_dims_the_bar(#[case] dim: bool) {
    let v = on_a_machine(dim, None);
    let strip = v.tab_bar.as_ref().expect("a bar").strip(bar());
    let t = render(&v);
    let dimmed = |r: Rect| t.backend().buffer()[(r.x + 1, r.y)].modifier.contains(Modifier::DIM);
    assert_eq!((dimmed(strip.item(1)), dimmed(strip.new_button())), (dim, dim));
}

#[test]
fn a_machine_that_is_not_linked_says_when_its_agent_was_seen() {
    let t = render(&on_a_machine(true, Some("2 min")));
    let r = bar();
    assert_eq!(row_text(&t, Rect { y: r.y + 1, height: 1, ..r }).trim(), "working · seen 2 min ago");
}

#[test]
fn a_machine_s_project_alone_has_its_bar() {
    let v = View { has_project: false, remote_project: true, ..on_a_machine(false, None) };
    let t = render(&v);
    let text = row_text(&t, bar());
    assert!(text.contains("claude") && text.contains("zsh"), "{text:?}");
}
