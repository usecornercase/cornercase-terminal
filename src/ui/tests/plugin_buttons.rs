use super::*;
use crate::ui::chrome::{PluginRow, plugin_buttons};

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

fn face(title: &str) -> Face {
    Face { title: title.into(), icon: title.chars().next().map(String::from).unwrap_or_default() }
}

fn with_buttons(titles: &[&str], sidebar: Sidebar) -> View<'static> {
    View { plugin_buttons: titles.iter().map(|t| face(t)).collect(), sidebar, ..view(&["~", "shop"]) }
}

fn screen_for(sidebar: Sidebar, plugins: bool, icon_bar: bool) -> Screen {
    Screen { sidebar, icon_bar, plugins, ..Screen::default() }
}

mod the_row {
    use super::*;

    #[rstest]
    #[case::side_by_side(Sidebar::SideBySide)]
    #[case::projects_on_top(Sidebar::ProjectsOnTop)]
    #[case::tree(Sidebar::Tree)]
    fn is_not_there_without_buttons(#[case] sidebar: Sidebar) {
        let a = full_layout(TALL, Widths::default(), screen_for(sidebar, false, false));
        assert!(a.plugins.is_empty());
        assert_eq!(a, layout_with(TALL, Widths::default(), false, sidebar));
    }

    #[rstest]
    #[case::side_by_side(Sidebar::SideBySide)]
    #[case::projects_on_top(Sidebar::ProjectsOnTop)]
    #[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
    #[case::tree(Sidebar::Tree)]
    fn sits_between_the_separator_and_settings(#[case] sidebar: Sidebar) {
        let a = full_layout(TALL, Widths::default(), screen_for(sidebar, true, false));
        assert_eq!((a.plugins.height, a.plugins.y + 1, a.settings.y), (1, a.settings.y, a.plugins.bottom()));
        assert!(a.separator.y < a.plugins.y, "{a:?}");
    }

    #[rstest]
    #[case::side_by_side(Sidebar::SideBySide)]
    #[case::projects_on_top(Sidebar::ProjectsOnTop)]
    fn takes_a_row_from_the_lists(#[case] sidebar: Sidebar) {
        let without = full_layout(TALL, Widths::default(), screen_for(sidebar, false, false));
        let with = full_layout(TALL, Widths::default(), screen_for(sidebar, true, false));
        assert_eq!(with.separator.y + 1, without.separator.y);
    }

    #[test]
    fn keeps_the_footers_of_both_columns_level() {
        let a = full_layout(TALL, Widths::default(), screen_for(Sidebar::SideBySide, true, false));
        assert_eq!(a.separator.y, a.workspaces_separator.y);
    }

    #[test]
    fn goes_with_the_icon_bar() {
        let a = full_layout(TALL, Widths::default(), screen_for(Sidebar::SideBySide, true, true));
        assert!(a.plugins.is_empty());
    }

    #[test]
    fn sits_above_the_footer_in_the_compact_projects_menu() {
        let a = full_layout(SMALL, Widths::default(), screen_for(Sidebar::SideBySide, true, false));
        assert_eq!((a.plugins.height, a.plugins.bottom()), (COMPACT_PITCH, a.settings.y));
    }

    #[test]
    fn hides_outside_the_compact_projects_menu() {
        let a = full_layout(SMALL, Widths::default(), screen_for(Sidebar::SideBySide, true, false));
        assert!(a.shown(Some(Nav::Workspaces)).plugins.is_empty());
    }
}

mod the_buttons {
    use super::*;

    const ROW: Rect = Rect { x: 0, y: 5, width: 20, height: 1 };

    fn widths(row: &PluginRow) -> Vec<u16> {
        row.buttons.iter().map(|b| b.width).collect()
    }

    #[test]
    fn line_up_after_the_indent_when_they_fit() {
        let row = plugin_buttons(ROW, ["Deploy", "Logs"]);
        assert_eq!(row.buttons.iter().map(|b| b.x).collect::<Vec<_>>(), vec![1, 9]);
        assert_eq!((widths(&row), row.more), (vec![8, 6], Rect::default()));
    }

    #[test]
    fn leave_the_last_slot_to_the_more_button_when_they_do_not_fit() {
        let row = plugin_buttons(ROW, ["Deploy", "Logs", "Preview"]);
        assert_eq!(widths(&row), vec![8, 6]);
        assert_eq!(row.more, Rect { x: 15, width: 3, ..ROW });
    }

    #[test]
    fn cut_a_single_button_wider_than_the_row() {
        let row = plugin_buttons(ROW, ["A very long plugin title"]);
        assert_eq!((widths(&row), row.more), (vec![18], Rect::default()));
    }

    #[test]
    fn keep_the_first_button_when_it_alone_overflows() {
        let row = plugin_buttons(ROW, ["A very long plugin title", "Logs"]);
        assert_eq!((widths(&row), row.more), (vec![15], Rect { x: 16, width: 3, ..ROW }));
    }

    #[test]
    fn are_none_in_an_empty_row() {
        assert_eq!(plugin_buttons(Rect::default(), ["Deploy"]), PluginRow::default());
    }
}

mod drawing {
    use super::*;

    #[test]
    fn renders_one_button() {
        insta::assert_snapshot!(render_sized(&with_buttons(&["Deploy"], Sidebar::SideBySide), W, H).backend());
    }

    #[test]
    fn renders_the_more_button_when_they_do_not_fit() {
        let v = with_buttons(&["Deploy", "Logs", "Preview", "Database"], Sidebar::SideBySide);
        insta::assert_snapshot!(render_sized(&v, W, H).backend());
    }

    #[test]
    fn renders_the_row_in_one_column() {
        let v = with_buttons(&["Deploy", "Logs"], Sidebar::ProjectsOnTop);
        insta::assert_snapshot!(render_sized(&v, W, TALL.height).backend());
    }

    #[test]
    fn lights_a_hovered_button() {
        let r = plugin_buttons(
            full_layout(AREA, Widths::default(), screen_for(Sidebar::SideBySide, true, false)).plugins,
            ["Deploy"],
        )
        .buttons[0];
        let v = View { hover: Some(r.as_position()), ..with_buttons(&["Deploy"], Sidebar::SideBySide) };
        assert_eq!(render(&v).backend().buffer()[r.as_position()].bg, Color::Cyan);
    }

    #[test]
    fn leaves_the_icon_bar_alone() {
        let v = View { icon_bar: true, icons: true, ..with_buttons(&["Deploy"], Sidebar::SideBySide) };
        let t = render(&v);
        let text: String = t.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
        assert!(!text.contains("Deploy"), "{text}");
    }
}

mod icon_bar {
    use super::*;
    use crate::ui::activity::{Item, Shown, WIDTH, items};

    fn with_bar(hover: Option<Position>) -> View<'static> {
        let deploy = Face { title: "Deploy".into(), icon: "◆".into() };
        View {
            icon_bar: true,
            icons: true,
            has_project: true,
            hover,
            plugin_buttons: vec![deploy, face("logs")],
            ..view(&["shop"])
        }
    }

    fn places(height: u16, shown: Shown) -> Vec<(Item, u16)> {
        items(Rect::new(0, 0, WIDTH, height), shown).into_iter().map(|(item, r)| (item, r.y)).collect()
    }

    #[test]
    fn puts_a_slot_for_each_plugin_after_flow() {
        let shown = Shown { flow: true, plugins: 2, ..Shown::default() };
        let top: Vec<(Item, u16)> = places(24, shown).into_iter().filter(|(_, y)| *y < 12).collect();
        assert_eq!(top, [(Item::Todo, 1), (Item::Flow, 3), (Item::Plugin(0), 5), (Item::Plugin(1), 7)]);
    }

    #[test]
    fn drops_the_plugins_first_when_the_bar_is_short() {
        let shown = Shown { plugins: 2, ..Shown::default() };
        let top: Vec<(Item, u16)> = places(12, shown).into_iter().filter(|(_, y)| *y < 4).collect();
        assert_eq!(top, [(Item::Todo, 1)]);
    }

    #[test]
    fn renders_the_icon_or_the_initial() {
        insta::assert_snapshot!(render_sized(&with_bar(None), W, 24).backend());
    }

    #[test]
    fn names_a_hovered_plugin_with_its_title() {
        let t = render_sized(&with_bar(Some(Position::new(1, 5))), W, 24);
        assert!(row_text(&t, Rect::new(WIDTH, 5, 10, 1)).starts_with(" Deploy "));
    }
}

mod compact {
    use super::*;

    #[test]
    fn renders_the_row_in_the_projects_menu() {
        let v = View { nav: Some(Nav::Projects), ..with_buttons(&["Deploy", "Logs"], Sidebar::SideBySide) };
        insta::assert_snapshot!(render_sized(&v, SMALL.width, SMALL.height).backend());
    }
}
