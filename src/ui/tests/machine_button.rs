use super::*;
use crate::ui::activity::{Item, Shown, items};

fn on(machine: &str) -> View<'static> {
    View { machine: Some(machine.into()), ..view(&["shop"]) }
}

fn with_machine(area: Rect, sidebar: Sidebar) -> Areas {
    full_layout(area, Widths::default(), Screen { sidebar, machine: true, ..Screen::default() })
}

fn row() -> Rect {
    with_machine(AREA, Sidebar::SideBySide).machine
}

mod on_its_own_row {
    use super::*;

    #[test]
    fn the_machine_sits_on_a_row_of_its_own_under_worktrees() {
        let t = render(&on("devbox"));
        let a = with_machine(AREA, Sidebar::SideBySide);

        assert_eq!(a.machine.y, a.worktrees.y + 1);
        assert_eq!(row_text(&t, a.machine).trim_end(), "  devbox ▴");
        assert_eq!(row_text(&t, a.worktrees).trim_end(), "  Worktrees      Recap");
    }

    #[test]
    fn renders_the_footer_with_the_machine() {
        insta::assert_snapshot!(render(&on("this machine")).backend());
    }

    #[test]
    fn a_window_that_knows_no_other_machine_has_the_rows_it_had() {
        let without = layout(AREA, Widths::default());
        let with = with_machine(AREA, Sidebar::SideBySide);

        assert_eq!(without.machine.height, 0);
        assert_eq!(with.more.y, without.more.y);
        assert_eq!(with.list.height + 1, without.list.height);
    }

    #[test]
    fn a_long_name_is_cut_and_keeps_its_mark() {
        let (r, text) = machine_button(row(), "me@devbox.internal.example.com", 0);

        assert!(text.ends_with("… ▴") && r.right() < row().right(), "{text:?} in {r:?}");
    }

    #[test]
    fn a_row_too_narrow_leaves_it_out() {
        let narrow = Rect { width: 7, ..row() };

        assert_eq!(machine_button(narrow, "devbox", 0), (Rect::default(), String::new()));
    }

    #[test]
    fn the_narrowest_row_that_shows_it_keeps_its_mark_and_cuts_the_name() {
        let shown = machine_button(Rect { width: 8, ..row() }, "devbox", 0);
        let hidden = machine_button(Rect { width: 7, ..row() }, "devbox", 0);

        assert_eq!((shown.1.as_str(), hidden.1.as_str()), ("d… ▴", ""));
        assert!(shown.0.right() <= row().x + 8 && hidden.0.is_empty());
    }

    #[test]
    fn the_count_of_tunnels_comes_before_the_mark() {
        assert_eq!(machine_button(row(), "devbox", 2).1, "devbox ⇄2 ▴");
    }

    #[test]
    fn a_long_name_with_tunnels_is_cut_and_keeps_the_count_whole() {
        let (r, text) = machine_button(row(), "me@devbox.internal.example.com", 2);

        assert!(text.ends_with("… ⇄2 ▴") && r.right() <= row().right(), "{text:?} in {r:?}");
    }

    #[test]
    fn the_narrowest_row_that_shows_the_count_keeps_it_whole() {
        let narrowest = (8..40)
            .find(|width| machine_button(Rect { width: *width, ..row() }, "devbox", 2).1.contains('⇄'))
            .expect("a width shows it");

        let shown = machine_button(Rect { width: narrowest, ..row() }, "devbox", 2);
        let less = machine_button(Rect { width: narrowest - 1, ..row() }, "devbox", 2);

        assert_eq!(shown.1, "d… ⇄2 ▴");
        assert!(less.1.ends_with("… ▴") && !less.1.contains('⇄'), "{:?}", less.1);
        assert_eq!(machine_button(Rect { width: 7, ..row() }, "devbox", 2).1, "");
    }

    #[test]
    fn renders_the_footer_with_tunnels_and_a_cut_name() {
        let view =
            View { machine: Some("me@devbox.internal.example.com".into()), machine_tunnels: 2, ..view(&["shop"]) };

        insta::assert_snapshot!(render(&view).backend());
    }

    #[test]
    fn hovering_it_lights_it_and_not_worktrees() {
        let (machine, _) = machine_button(row(), "devbox", 0);
        let view = View { hover: Some(Position::new(machine.x + 1, machine.y)), ..on("devbox") };

        let t = render(&view);

        let lit = |x, y| t.backend().buffer()[(x, y)].bg == Color::Cyan;
        let worktrees = with_machine(AREA, Sidebar::SideBySide).worktrees;
        assert_eq!((lit(machine.x + 1, machine.y), lit(worktrees.x + 2, worktrees.y)), (true, false));
    }

    #[rstest]
    #[case::stacked(Sidebar::ProjectsOnTop)]
    #[case::tree(Sidebar::Tree)]
    fn the_stacked_footer_gains_one_row_for_it(#[case] sidebar: Sidebar) {
        let tall = Rect { x: 0, y: 0, width: W, height: 30 };
        let with = with_machine(tall, sidebar);
        let without = layout_with(tall, Widths::default(), false, sidebar);

        assert_eq!(with.machine.y, with.worktrees.y + 1);
        assert_eq!(with.more.y, without.more.y);
        assert_eq!(without.machine.height, 0);
    }

    #[test]
    fn the_compact_menu_gets_a_band_for_it_below_worktrees() {
        let compact = full_layout(SMALL, Widths::default(), Screen { machine: true, ..Screen::default() });
        let band = compact.shown(Some(Nav::Projects)).machine;

        let (r, text) = machine_button(band, "devbox", 0);

        assert_eq!((r.y, r.height, text.as_str()), (band.y, band.height, "devbox ▴"));
        assert!(band.y > compact.worktrees.y);
    }
}

mod icon_bar {
    use super::*;

    fn shown(machine: bool) -> Shown {
        Shown { issues: false, changes: false, files: true, flow: false, plugins: 0, machine }
    }

    fn bottom(machine: bool) -> Vec<(Item, u16)> {
        let bar = Rect::new(0, 0, crate::ui::activity::WIDTH, 24);
        items(bar, shown(machine)).into_iter().map(|(item, r)| (item, r.y)).filter(|(_, y)| *y > 12).collect()
    }

    #[test]
    fn the_machine_sits_above_help_once_another_machine_is_known() {
        assert_eq!(bottom(true), [(Item::More, 22), (Item::Help, 20), (Item::Machine, 18)]);
    }

    #[test]
    fn no_slot_is_kept_for_it_otherwise() {
        assert_eq!(bottom(false), [(Item::More, 22), (Item::Help, 20)]);
    }
}
