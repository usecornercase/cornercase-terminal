use super::pane::split_footer;
use super::*;
use crate::project::split::MIN_ROWS;

const LEAF: Rect = Rect { x: 4, y: 2, width: 30, height: 10 };

#[rstest]
#[case::one_row(1)]
#[case::two_rows(2)]
#[case::three_rows(3)]
#[case::a_tall_pane(40)]
fn without_a_footer_the_pane_is_left_whole(#[case] height: u16) {
    let leaf = Rect { height, ..LEAF };

    assert_eq!(split_footer(leaf, false), (leaf, None));
}

#[test]
fn a_one_row_pane_keeps_its_row_for_the_terminal() {
    let leaf = Rect { height: 1, ..LEAF };

    assert_eq!(split_footer(leaf, true), (leaf, None));
}

#[rstest]
#[case::two_rows(2)]
#[case::three_rows(3)]
#[case::a_tall_pane(40)]
fn the_footer_takes_the_last_row(#[case] height: u16) {
    let leaf = Rect { height, ..LEAF };

    let (terminal, row) = split_footer(leaf, true);

    assert_eq!(terminal, Rect { height: height - 1, ..leaf });
    assert_eq!(row, Some(Rect::new(leaf.x, leaf.bottom() - 1, leaf.width, 1)));
}

#[test]
fn the_smallest_pane_leaves_two_rows_to_its_terminal() {
    let (terminal, _) = split_footer(Rect { height: MIN_ROWS, ..LEAF }, true);

    assert_eq!(terminal.height, 2);
}

mod the_row_of_accounts {
    use ratatui::style::Modifier;

    use super::*;
    use crate::project::split::{Dir, Node};
    use crate::ui::accounts_bar::draw_row;

    fn pill(agent: &str, account: &str) -> AccountPill {
        AccountPill { agent: agent.into(), account: account.into() }
    }

    fn shell() -> AccountRow {
        AccountRow { pills: vec![pill("claude", "personal"), pill("codex", "System")], blocked: false }
    }

    fn agent(account: &str) -> AccountRow {
        AccountRow { pills: vec![pill("claude", account)], blocked: false }
    }

    fn split(accounts: Vec<AccountRow>) -> TabView {
        let mut layout = Node::Leaf(0);
        layout.split(0, Dir::Right, 1);
        layout.split(1, Dir::Down, 2);
        let screens = vec![screen(b"$ claude"), screen(b"$ claude"), screen(b"$ vim")];
        TabView { layout, screens, active: 0, footer: true, accounts, ..single(screen(b"")) }
    }

    fn one(row: AccountRow) -> View<'static> {
        View { tab: Some(TabView { footer: true, accounts: vec![row], ..single(screen(b"$ ")) }), ..view(&["~"]) }
    }

    fn panes(v: &View, width: u16, height: u16) -> Vec<Rect> {
        let area = layout(Rect::new(0, 0, width, height), Widths::default()).pane;
        let tab = v.tab.as_ref().expect("a tab");
        tab.layout.visible(area, tab.active).into_iter().map(|(_, leaf)| leaf).collect()
    }

    fn row_of(leaf: Rect) -> Rect {
        split_footer(leaf, true).1.expect("a row")
    }

    fn pill_cell(v: &View, pane: usize, pill: usize) -> Position {
        let row = row_of(panes(v, W, H)[pane]);
        let tab = v.tab.as_ref().expect("a tab");
        let (_, r, _) = pill_rects(row, &tab.accounts[pane].pills).swap_remove(pill);
        Position::new(r.x + 1, r.y)
    }

    fn row_alone(row: &AccountRow, width: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(width, 1)).expect("test backend");
        let v = view(&["~"]);
        t.draw(|f| draw_row(f, &v, Rect::new(0, 0, width, 1), row, false)).expect("draw");
        row_text(&t, Rect::new(0, 0, width, 1))
    }

    #[test]
    fn a_shell_shows_a_button_per_agent_under_its_terminal() {
        insta::assert_snapshot!(render(&one(shell())).backend());
    }

    #[test]
    fn in_compact_mode_the_row_sits_under_the_pane_too() {
        insta::assert_snapshot!(render_sized(&one(shell()), SMALL.width, SMALL.height).backend());
    }

    #[test]
    fn each_pane_of_a_split_shows_its_own_account() {
        let v = View {
            tab: Some(split(vec![agent("work"), agent("personal"), AccountRow { blocked: true, ..shell() }])),
            ..view(&["~"])
        };
        insta::assert_snapshot!(render(&v).backend());
    }

    #[rstest]
    #[case::whole(40, "  claude: personal ▾   codex: System ▾  ")]
    #[case::short(26, "  personal ▾   System ▾   ")]
    #[case::the_first_one(16, "  personal ▾    ")]
    #[case::cut(10, "  pers… ▾ ")]
    #[case::nothing(6, "      ")]
    fn a_narrow_row_shortens_then_drops_then_cuts(#[case] width: u16, #[case] drawn: &str) {
        assert_eq!(row_alone(&shell(), width), drawn);
    }

    #[test]
    fn the_row_of_an_inactive_pane_is_dimmed_like_its_pane() {
        let v = View { tab: Some(split(vec![agent("work"), agent("personal"), shell()])), ..view(&["~"]) };
        let t = render(&v);
        let style = |at: Position| t.backend().buffer()[(at.x, at.y)].style();
        assert!(!style(pill_cell(&v, 0, 0)).add_modifier.contains(Modifier::DIM));
        assert!(style(pill_cell(&v, 1, 0)).add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn the_buttons_of_a_pane_running_another_program_are_dimmed() {
        let v = one(AccountRow { blocked: true, ..shell() });
        let t = render(&v);
        let at = pill_cell(&v, 0, 0);
        assert!(t.backend().buffer()[(at.x, at.y)].style().add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn hovering_lights_only_the_button_under_the_pointer() {
        let v = View { hover: Some(pill_cell(&one(shell()), 0, 0)), ..one(shell()) };
        let t = render(&v);
        let bg = |pill| {
            let at = pill_cell(&v, 0, pill);
            t.backend().buffer()[(at.x, at.y)].style().bg
        };
        assert_eq!(bg(0), Some(Color::Cyan));
        assert_ne!(bg(1), Some(Color::Cyan));
    }

    #[test]
    fn without_a_footer_the_rows_are_not_drawn() {
        let v = View {
            tab: Some(TabView { footer: false, accounts: vec![shell()], ..single(screen(b"$ ")) }),
            ..view(&["~"])
        };
        let row = Rect { y: panes(&v, W, H)[0].bottom() - 1, height: 1, ..panes(&v, W, H)[0] };
        assert!(!row_text(&render(&v), row).contains("claude"));
    }

    #[test]
    fn a_tab_too_small_for_its_split_shows_the_active_pane_with_its_row() {
        let v = View { tab: Some(split(vec![agent("work"), agent("personal"), shell()])), ..view(&["~"]) };
        let tab = v.tab.as_ref().expect("a tab");
        let room = |h| tab.layout.has_room(layout(Rect::new(0, 0, W, h), Widths::default()).pane);
        let height = (1..H).rev().find(|&h| !room(h)).expect("a height without room");
        let area = layout(Rect::new(0, 0, W, height), Widths::default()).pane;
        assert!(!tab.layout.has_room(area) && area.height >= MIN_ROWS, "{area:?}");
        let shown = tab.layout.visible(area, tab.active);
        assert_eq!(shown, [(0, area)]);

        let t = render_sized(&v, W, height);

        assert!(row_text(&t, row_of(area)).contains("claude: work"), "{}", row_text(&t, row_of(area)));
        assert!(!row_text(&t, row_of(area)).contains("personal"));
    }

    #[rstest]
    #[case::resuming(PaneBand::Resuming("resuming claude".into()), "resuming claude")]
    #[case::restarting(PaneBand::Restarts("restart in 3s".into()), "restart in 3s")]
    #[case::held(PaneBand::Held { text: "2 held".into(), queued: false }, "send")]
    fn the_bands_and_the_scrollback_badge_sit_on_the_terminal_s_last_row(#[case] band: PaneBand, #[case] text: &str) {
        let mut shown = screen(b"$ ");
        shown.below = 7;
        let v = View {
            tab: Some(TabView {
                footer: true,
                accounts: vec![shell()],
                bands: vec![Some(band)],
                screens: vec![shown],
                ..single(screen(b"$ "))
            }),
            ..view(&["~"])
        };
        let t = render(&v);
        let leaf = panes(&v, W, H)[0];
        let (terminal, footer) = split_footer(leaf, true);
        let line = |y: u16| row_text(&t, Rect::new(leaf.x, y, leaf.width, 1));

        let last = line(terminal.bottom() - 1);
        assert!(last.contains("7 more"), "{last}");
        let inside: String = (terminal.y..terminal.bottom()).map(line).collect();
        assert!(inside.contains(text), "{inside}");
        let footer = line(footer.expect("a row").y);
        assert!(footer.contains("claude: personal") && !footer.contains("more") && !footer.contains(text), "{footer}");
    }
}
