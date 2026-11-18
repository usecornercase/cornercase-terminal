use super::*;

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

fn entry(name: &str, detail: &str) -> BotEntry {
    BotEntry {
        name: name.into(),
        detail: detail.into(),
        fallback: String::new(),
        thinking: false,
        unread: 0,
        active: false,
        asking: false,
    }
}

fn entries() -> Vec<BotEntry> {
    vec![
        BotEntry { unread: 2, ..entry("prs", "claude · opus") },
        BotEntry { thinking: true, ..entry("digest", "codex") },
        entry("inbox", "claude · haiku"),
    ]
}

fn shown(sidebar: Sidebar, entries: Vec<BotEntry>, folded: bool) -> View<'static> {
    View {
        has_project: true,
        sidebar,
        spin: Some(0),
        bots: Some(BotsView { entries, scroll: 0, folded }),
        ..view(&["cornercase", "website", "api"])
    }
}

fn with_bots(sidebar: Sidebar, bots: u16, agents: bool) -> Areas {
    full_layout(TALL, Widths::default(), Screen { sidebar, agents, bots, ..Screen::default() })
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::projects_on_top(Sidebar::ProjectsOnTop)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
#[case::tree(Sidebar::Tree)]
fn no_bots_take_no_room(#[case] sidebar: Sidebar) {
    let before = full_layout(TALL, Widths::default(), Screen { sidebar, agents: true, ..Screen::default() });
    assert_eq!(with_bots(sidebar, 0, true), before);
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide)]
#[case::projects_on_top(Sidebar::ProjectsOnTop)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
#[case::tree(Sidebar::Tree)]
fn sit_above_the_agents_and_below_the_lists(#[case] sidebar: Sidebar) {
    let a = with_bots(sidebar, 3, true);
    let clear = |r: Rect| !r.intersects(a.bots) && !r.intersects(a.bots_line);
    assert_eq!(
        (clear(a.list), clear(a.workspaces_list), a.bots_title.y, a.bots.bottom() <= a.agents_border.y),
        (true, true, a.bots_line.bottom(), true)
    );
}

#[test]
fn a_section_without_agents_ends_above_the_footer() {
    let a = with_bots(Sidebar::SideBySide, 3, false);
    assert_eq!((a.bots.height, a.bots.bottom() <= a.separator.y), (6, true));
}

#[test]
fn leave_the_pane_as_it_was() {
    assert_eq!(with_bots(Sidebar::SideBySide, 3, true).pane, with_bots(Sidebar::SideBySide, 0, true).pane);
}

#[test]
fn folded_is_the_title_row_alone() {
    let a = full_layout(TALL, Widths::default(), Screen { bots: 3, bots_folded: true, ..Screen::default() });
    assert_eq!((a.bots.height, a.bots_list.is_empty()), (1, true));
}

#[test]
fn the_height_is_capped_at_half_the_room() {
    let a = with_bots(Sidebar::SideBySide, 40, false);
    assert!(a.bots.height <= (a.list.height + a.bots.height + 2) / 2 + 1);
    assert!(a.list.height >= MIN_STACK_SECTION - 2);
}

#[test]
fn a_room_too_small_for_the_title_has_no_section() {
    let tiny = Rect { height: 6, ..TALL };
    let a = full_layout(tiny, Widths::default(), Screen { bots: 3, ..Screen::default() });
    assert!(a.bots.is_empty());
}

#[test]
fn the_stack_line_cannot_be_dragged_into_it() {
    let screen = Screen { bots: 3, ..Screen::default() };
    let widths = Widths::default().stacked_dragged(Border::Stack, Position::new(5, 28), TALL, screen);
    let a = full_layout(TALL, widths, Screen { sidebar: Sidebar::ProjectsOnTop, ..screen });
    assert!(a.workspaces_list.bottom() <= a.bots_line.y);
}

#[test]
fn a_click_on_a_row_hits_that_bot() {
    let a = with_bots(Sidebar::SideBySide, 3, false);
    let row = bot_rows(a.bots_list, 1, 3, 0).item(1);
    assert_eq!(bot_rows(a.bots_list, 1, 3, 0).at(Position::new(row.x + 4, row.y)), Some(1));
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide, "side_by_side")]
#[case::projects_on_top(Sidebar::ProjectsOnTop, "projects_on_top")]
#[case::tree(Sidebar::Tree, "tree")]
fn the_section_is_drawn(#[case] sidebar: Sidebar, #[case] name: &str) {
    let drawn = render_sized(&shown(sidebar, entries(), false), W, TALL.height);
    insta::assert_snapshot!(format!("the_section_is_drawn_{name}"), drawn.backend());
}

#[test]
fn the_shown_bot_gets_the_rail_and_the_surface() {
    let mut list = entries();
    list[0].active = true;
    let a = with_bots(Sidebar::SideBySide, 3, false);
    let row = bot_rows(a.bots_list, 1, 3, 0).item(0);
    let t = render_sized(&shown(Sidebar::SideBySide, list, false), W, TALL.height);
    let cell = &t.backend().buffer()[(row.x, row.y)];
    assert_eq!((cell.symbol(), cell.fg, cell.bg), ("▌", Color::Cyan, DARK_SURFACE));
}

#[test]
fn folded_keeps_the_unread_count_and_the_spinner_on_the_title() {
    insta::assert_snapshot!(render_sized(&shown(Sidebar::SideBySide, entries(), true), W, TALL.height).backend());
}

#[test]
fn a_hovered_row_shows_its_menu_button() {
    let a = with_bots(Sidebar::SideBySide, 3, false);
    let row = bot_rows(a.bots_list, 1, 3, 0).item(2);
    let v = View { hover: Some(Position::new(row.x + 4, row.y)), ..shown(Sidebar::SideBySide, entries(), false) };
    let t = render_sized(&v, W, TALL.height);
    assert!(row_text(&t, row).contains('⋯'));
}

#[test]
fn a_row_shows_no_close_button() {
    let a = with_bots(Sidebar::SideBySide, 3, false);
    let row = bot_rows(a.bots_list, 1, 3, 0).item(2);
    let v = View { hover: Some(Position::new(row.x + 4, row.y)), ..shown(Sidebar::SideBySide, entries(), false) };
    assert!(!row_text(&render_sized(&v, W, TALL.height), row).contains('×'));
}

#[test]
fn a_narrow_row_drops_the_details_first() {
    let widths = Widths { projects: 22, ..Widths::default() };
    let a = full_layout(TALL, widths, Screen { sidebar: Sidebar::SideBySide, bots: 3, ..Screen::default() });
    let v = View { widths, ..shown(Sidebar::SideBySide, entries(), false) };
    let row = bot_rows(a.bots_list, 1, 3, 0).item(2);
    let t = render_sized(&v, W, TALL.height);
    let text = row_text(&t, row);
    assert!(text.contains("inbox") && !text.contains("claude"), "{text}");
}

fn compact_view(nav: Option<Nav>) -> View<'static> {
    View { nav, ..shown(Sidebar::SideBySide, entries(), false) }
}

fn compact_areas(nav: Option<Nav>, agents: bool) -> Areas {
    full_layout(SMALL, Widths::default(), Screen { bots: 3, agents, ..Screen::default() }).shown(nav)
}

#[rstest]
#[case::with_agents(true)]
#[case::without_agents(false)]
fn the_projects_menu_has_a_bots_button_beside_the_agents(#[case] agents: bool) {
    let a = compact_areas(Some(Nav::Projects), agents);
    assert!(!a.bots_button.is_empty() && a.bots.is_empty());
    assert_eq!(a.agents_button.is_empty(), !agents);
    if agents {
        assert!(a.bots_button.right() <= a.agents_button.x);
    }
}

#[test]
fn the_bots_page_hides_the_projects_and_lists_the_bots_on_bands() {
    let a = compact_areas(Some(Nav::Bots), false);
    assert!(a.list.is_empty() && a.bots_button.is_empty() && !a.back.is_empty());
    assert_eq!(bot_rows(a.bots_list, a.pitch, 3, 0).item(1).height, COMPACT_PITCH);
}

#[test]
fn no_bots_leave_the_projects_menu_as_it_was() {
    let plain = full_layout(SMALL, Widths::default(), Screen::default()).shown(Some(Nav::Projects));
    assert!(plain.bots_button.is_empty());
}

#[test]
fn the_projects_menu_shows_the_bots_button() {
    insta::assert_snapshot!(render_sized(&compact_view(Some(Nav::Projects)), 80, 30).backend());
}

#[test]
fn the_bots_page_is_drawn() {
    insta::assert_snapshot!(render_sized(&compact_view(Some(Nav::Bots)), 80, 30).backend());
}

#[test]
fn a_row_too_narrow_for_the_next_run_draws_agent_and_model_instead() {
    let long = BotEntry { detail: "next 16 Nov 09:00".into(), fallback: "claude".into(), ..entry("triage-bot-x", "") };
    let short = BotEntry { detail: "next Mon 09:00".into(), fallback: "claude".into(), ..entry("prs", "") };
    let t = render_sized(&shown(Sidebar::SideBySide, vec![long, short], false), W, TALL.height);
    let text: String = (0..TALL.height).map(|y| row_text(&t, Rect::new(0, y, W, 1))).collect::<Vec<_>>().join("\n");
    assert!(text.contains("claude"), "{text}");
    assert!(text.contains("next Mon 09:00"), "{text}");
    assert!(!text.contains("next 16 Nov"), "{text}");
}
