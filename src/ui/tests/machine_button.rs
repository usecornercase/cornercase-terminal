use super::*;
use crate::ui::activity::{Item, Shown, items};

fn on(machine: &str) -> View<'static> {
    View { machine: Some(machine.into()), ..view(&["shop"]) }
}

fn row() -> Rect {
    areas().worktrees
}

mod beside_worktrees {
    use super::*;

    #[test]
    fn the_machine_sits_at_the_middle_of_the_worktrees_row() {
        let t = render(&on("devbox"));

        assert_eq!(row_text(&t, row()).trim_end(), "  Worktrees      devbox ▴");
    }

    #[test]
    fn renders_the_footer_with_the_machine() {
        insta::assert_snapshot!(render(&on("this machine")).backend());
    }

    #[test]
    fn a_long_name_is_cut_and_keeps_its_mark() {
        let (r, text) = machine_button(row(), "me@devbox.internal.example.com");

        assert!(text.ends_with("… ▴") && r.right() < row().right(), "{text:?} in {r:?}");
    }

    #[test]
    fn worktrees_keeps_the_cells_left_of_it() {
        let (machine, _) = machine_button(row(), "devbox");

        assert_eq!(worktrees_button(row(), Some(machine)), Rect { width: machine.x - row().x, ..row() });
    }

    #[test]
    fn a_column_too_narrow_leaves_it_out() {
        let narrow = Rect { width: 16, ..row() };

        assert_eq!(machine_button(narrow, "devbox"), (Rect::default(), String::new()));
        assert_eq!(worktrees_button(narrow, Some(Rect::default())), narrow);
    }

    #[test]
    fn hovering_it_lights_it_and_not_worktrees() {
        let (machine, _) = machine_button(row(), "devbox");
        let view = View { hover: Some(Position::new(machine.x + 1, machine.y)), ..on("devbox") };

        let t = render(&view);

        let lit = |x| t.backend().buffer()[(x, machine.y)].bg == Color::Cyan;
        assert_eq!((lit(machine.x + 1), lit(row().x + 2)), (true, false));
    }

    #[test]
    fn the_compact_menu_shares_the_worktrees_band_with_it() {
        let band = layout(SMALL, Widths::default()).shown(Some(Nav::Projects)).worktrees;

        let (r, text) = machine_button(band, "devbox");

        assert_eq!((r.y, r.height, text.as_str()), (band.y, band.height, "devbox ▴"));
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
