use super::*;
use crate::ui::layout::{full_layout, window_layout};

fn pill(agent: &str, account: &str) -> AccountPill {
    AccountPill { agent: agent.into(), account: account.into() }
}

fn with_accounts(sidebar: Sidebar, pills: Vec<AccountPill>) -> View<'static> {
    View { accounts: pills, sidebar, ..view(&["cornercase", "shop"]) }
}

fn two(sidebar: Sidebar) -> View<'static> {
    with_accounts(sidebar, vec![pill("claude", "work"), pill("codex", "System")])
}

fn snapshot(t: &Terminal<TestBackend>) -> String {
    format!("{}", t.backend())
}

fn areas_of(view: &View, area: Rect) -> Areas {
    let columns = |main| full_layout(main, view.widths, false, view.sidebar, false, view.icons);
    window_layout(area, !view.accounts.is_empty(), columns).shown(view.nav)
}

fn drawn(view: &View, area: Rect) -> Vec<String> {
    let t = render_sized(view, area.width, area.height);
    pill_rects(&areas_of(view, area), &view.accounts).into_iter().map(|(_, r, _)| row_text(&t, r)).collect()
}

#[rstest]
#[case::side_by_side(Sidebar::SideBySide, false, None, AREA)]
#[case::projects_on_top(Sidebar::ProjectsOnTop, false, None, AREA)]
#[case::tree(Sidebar::Tree, false, None, AREA)]
#[case::icon_bar(Sidebar::SideBySide, true, None, AREA)]
#[case::compact(Sidebar::SideBySide, false, None, SMALL)]
#[case::compact_menu(Sidebar::SideBySide, false, Some(Nav::Projects), SMALL)]
fn the_window_ends_with_a_status_bar_of_accounts(
    #[case] sidebar: Sidebar,
    #[case] icons: bool,
    #[case] nav: Option<Nav>,
    #[case] area: Rect,
) {
    let view = View { nav, icons, ..two(sidebar) };
    let areas = areas_of(&view, area);
    assert_eq!(areas.accounts, Rect { y: area.bottom() - 2, height: 2, ..area });
    let above = Rect { height: area.height - 2, ..area };
    let rest = full_layout(above, view.widths, false, sidebar, false, icons).shown(nav);
    assert_eq!(Areas { accounts: Rect::default(), ..areas }, rest);
    let name = format!("status_bar_{sidebar:?}_{icons}_{}_{}", area.width, nav.map_or("closed", |_| "menu"));
    insta::assert_snapshot!(name, snapshot(&render_sized(&view, area.width, area.height)));
}

#[test]
fn the_pills_are_drawn_where_they_are_hit() {
    let view = two(Sidebar::SideBySide);
    assert_eq!(drawn(&view, AREA), [" claude: work ▾ ", " codex: System ▾ "]);
    let areas = areas_of(&view, AREA);
    let rects = pill_rects(&areas, &view.accounts);
    for (i, r, _) in &rects {
        assert_eq!(account_pill_hit(&areas, &view.accounts, Position::new(r.right() - 1, r.y)), Some(*i));
    }
    let gap = Position::new(rects[0].1.right(), rects[0].1.y);
    assert_eq!(account_pill_hit(&areas, &view.accounts, gap), None);
}

#[test]
fn pills_that_do_not_fit_drop_the_agent_then_the_last_ones() {
    let narrow = |width| Rect { width, ..SMALL };
    assert_eq!(drawn(&two(Sidebar::SideBySide), narrow(30)), [" work ▾ ", " System ▾ "]);
    let three = with_accounts(
        Sidebar::SideBySide,
        vec![pill("claude", "work"), pill("codex", "Personal"), pill("gemini", "System")],
    );
    assert_eq!(drawn(&three, narrow(26)), [" work ▾ ", " Personal ▾ "]);
}

#[test]
fn a_long_label_alone_is_cut() {
    let long = with_accounts(Sidebar::SideBySide, vec![pill("claude", "Everything at work, all day")]);
    let shown = drawn(&long, Rect { width: 20, ..SMALL });
    assert_eq!(shown.len(), 1);
    assert!(shown[0].ends_with("… ▾ "), "{shown:?}");
}

#[test]
fn hovering_a_pill_lights_only_that_pill() {
    let view = two(Sidebar::SideBySide);
    let rects = pill_rects(&areas_of(&view, AREA), &view.accounts);
    let (first, second) = (rects[0].1, rects[1].1);
    let lit = View { hover: Some(first.as_position()), ..view };
    let t = render(&lit);
    let bg = |r: Rect| t.backend().buffer()[(r.x, r.y)].bg;
    assert_eq!(bg(first), Color::Cyan);
    assert_ne!(bg(second), Color::Cyan);
}
