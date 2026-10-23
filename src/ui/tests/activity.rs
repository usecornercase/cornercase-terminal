use super::*;
use crate::ui::activity::{Item, Shown, WIDTH, items};

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 24 };

fn everything() -> Shown {
    Shown { issues: true, changes: true, files: true, flow: false, update: true, remote: true, plugins: 0 }
}

fn with_bar(hover: Option<Position>) -> View<'static> {
    View {
        icon_bar: true,
        icons: true,
        has_project: true,
        issues: true,
        changes_button: Some(ChangesButton { label: "Changes 3".into(), files: 3, open: false }),
        todo_pending: true,
        update: Some("↑ 9.0.0".into()),
        remote: Some(true),
        hover,
        ..view(&["shop"])
    }
}

fn icons_at(t: &Terminal<TestBackend>) -> Vec<(u16, String)> {
    let buffer = t.backend().buffer();
    (0..buffer.area.height).map(|y| (y, buffer[(1, y)].symbol().to_string())).filter(|(_, s)| s != " ").collect()
}

fn places(bar: Rect, shown: Shown) -> Vec<(Item, u16)> {
    items(bar, shown).into_iter().map(|(item, r)| (item, r.y)).collect()
}

mod layout {
    use super::*;

    #[rstest]
    #[case::side_by_side(Sidebar::SideBySide)]
    #[case::projects_on_top(Sidebar::ProjectsOnTop)]
    #[case::tree(Sidebar::Tree)]
    fn takes_the_left_edge_and_the_buttons_under_the_lists_go(#[case] sidebar: Sidebar) {
        let a = full_layout(
            TALL,
            Widths::default(),
            Screen { changes: false, sidebar, agents: false, icon_bar: true, plugins: false },
        );
        let footer = [a.separator, a.settings, a.usage, a.quit, a.issues, a.todo_button, a.files_button];
        assert_eq!(
            (a.activity, a.sidebar.x, footer.iter().all(|r| r.is_empty())),
            (Rect::new(0, 0, WIDTH, TALL.height), WIDTH, true)
        );
    }

    #[test]
    fn the_lists_reach_the_bottom() {
        let without = full_layout(
            TALL,
            Widths::default(),
            Screen { changes: false, sidebar: Sidebar::SideBySide, agents: false, icon_bar: false, plugins: false },
        );
        let with = full_layout(
            TALL,
            Widths::default(),
            Screen { changes: false, sidebar: Sidebar::SideBySide, agents: false, icon_bar: true, plugins: false },
        );
        assert_eq!((without.list.bottom(), with.list.bottom()), (TALL.height - 4, TALL.height));
    }

    #[test]
    fn is_not_there_by_default() {
        assert!(areas().activity.is_empty());
    }

    #[test]
    fn compact_mode_keeps_its_bar() {
        let a = full_layout(
            SMALL,
            Widths::default(),
            Screen { changes: false, sidebar: Sidebar::SideBySide, agents: false, icon_bar: true, plugins: false },
        );
        assert_eq!((a.activity, a.compact()), (Rect::default(), true));
    }
}

mod places {
    use super::*;

    #[test]
    fn panels_go_at_the_top_and_the_rest_at_the_bottom() {
        let bar = Rect::new(0, 0, WIDTH, 24);
        assert_eq!(
            places(bar, everything()),
            [
                (Item::Quit, 22),
                (Item::Settings, 20),
                (Item::Help, 18),
                (Item::Usage, 16),
                (Item::Remote, 14),
                (Item::Update, 12),
                (Item::Issues, 1),
                (Item::Changes, 3),
                (Item::Files, 5),
                (Item::Todo, 7),
            ]
        );
    }

    #[test]
    fn a_hidden_button_leaves_no_gap() {
        let shown = Shown { changes: false, ..everything() };
        let top: Vec<(Item, u16)> =
            places(Rect::new(0, 0, WIDTH, 24), shown).into_iter().filter(|(_, y)| *y < 10).collect();
        assert_eq!(top, [(Item::Issues, 1), (Item::Files, 3), (Item::Todo, 5)]);
    }

    #[test]
    fn a_short_bar_drops_panels_before_the_bottom_buttons() {
        let shown = Shown::default();
        assert_eq!(
            places(Rect::new(0, 0, WIDTH, 9), shown),
            [(Item::Quit, 7), (Item::Settings, 5), (Item::Help, 3), (Item::Usage, 1)]
        );
    }

    #[test]
    fn a_press_on_a_button_names_it() {
        let bar = Rect::new(0, 0, WIDTH, 24);
        let hits: Vec<Option<Item>> = [(1, 3), (2, 22), (3, 22), (1, 4)]
            .map(|(x, y)| activity_hit(bar, everything(), Position::new(x, y)))
            .into();
        assert_eq!(hits, [Some(Item::Changes), Some(Item::Quit), None, None]);
    }
}

mod drawing {
    use super::*;

    #[test]
    fn renders_the_bar_with_its_badges() {
        insta::assert_snapshot!(render_sized(&with_bar(None), W, 24).backend());
    }

    #[test]
    fn an_open_panel_gets_the_rail() {
        let todo = todo::View {
            items: Vec::new(),
            scroll: 0,
            adding: None,
            light: false,
            muted: Color::DarkGray,
            drag: None,
            compact: false,
        };
        let v = View { todo: Some(todo), ..with_bar(None) };
        let t = render_sized(&v, W, 24);
        let buffer = t.backend().buffer();
        assert_eq!((buffer[(0, 7)].symbol(), buffer[(0, 7)].fg, buffer[(1, 7)].fg), ("▌", Color::Cyan, Color::Cyan));
    }

    #[test]
    fn counts_and_dots_sit_at_the_corner_of_their_icon() {
        let t = render_sized(&with_bar(None), W, 24);
        let buffer = t.backend().buffer();
        let corner = |y: u16| buffer[(2, y + 1)].symbol().to_string();
        assert_eq!([corner(3), corner(7), corner(14)], [icons::count(3), icons::DOT, icons::DOT]);
    }

    #[test]
    fn hovering_a_button_names_it_beside_the_bar() {
        let t = render_sized(&with_bar(Some(Position::new(1, 20))), W, 24);
        assert!(row_text(&t, Rect::new(WIDTH, 20, 12, 1)).starts_with(" Settings "));
    }

    #[test]
    fn nothing_is_named_while_a_dialog_is_open() {
        let search = Search {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            scroll: 0,
            hint: String::new(),
            sessions: false,
            searching: false,
        };
        let v = View { overlay: Some(Overlay::Search(search)), ..with_bar(Some(Position::new(1, 20))) };
        let t = render_sized(&v, W, 24);
        assert!(!row_text(&t, Rect::new(WIDTH, 20, 12, 1)).contains("Settings"));
    }

    #[test]
    fn every_button_draws_its_icon() {
        let t = render_sized(&with_bar(None), W, 24);
        let drawn: Vec<String> = icons_at(&t).into_iter().map(|(_, s)| s).collect();
        assert_eq!(
            drawn,
            [
                icons::ISSUES,
                icons::NERD.changes,
                icons::NERD.files,
                icons::NERD.todo,
                icons::UPDATE,
                icons::REMOTE,
                icons::USAGE,
                icons::HELP,
                icons::SETTINGS,
                icons::QUIT
            ]
        );
    }

    #[test]
    fn the_compact_bar_keeps_plain_symbols_when_icons_are_off() {
        let v = View { icon_bar: true, icons: false, has_project: true, ..with_bar(None) };
        let t = render_sized(&v, SMALL.width, SMALL.height);
        let a = full_layout(
            SMALL,
            Widths::default(),
            Screen { changes: false, sidebar: Sidebar::SideBySide, agents: false, icon_bar: true, plugins: false },
        );
        let middle = |r: Rect| row_text(&t, Rect { y: r.y + 1, ..r }).trim().to_string();
        assert_eq!(
            [middle(a.changes_button), middle(a.files_button), middle(a.search_button)],
            [icons::PLAIN.changes, icons::PLAIN.files, icons::PLAIN.search]
        );
    }

    #[test]
    fn the_compact_bar_draws_the_same_icons() {
        let v = View { icon_bar: true, icons: true, has_project: true, ..with_bar(None) };
        let t = render_sized(&v, SMALL.width, SMALL.height);
        let a = full_layout(
            SMALL,
            Widths::default(),
            Screen { changes: false, sidebar: Sidebar::SideBySide, agents: false, icon_bar: true, plugins: false },
        );
        let middle = |r: Rect| row_text(&t, Rect { y: r.y + 1, ..r }).trim().to_string();
        assert_eq!(
            [middle(a.changes_button), middle(a.files_button), middle(a.search_button)],
            [icons::NERD.changes, icons::NERD.files, icons::NERD.search]
        );
    }

    #[test]
    fn counts_above_nine_read_nine_plus() {
        assert_eq!([icons::count(1), icons::count(9), icons::count(12)], ["\u{f0ca0}", "\u{f0cb0}", "\u{f0cb2}"]);
    }
}

mod colours {
    use libghostty_vt::style::RgbColor;

    use super::*;

    const SETTINGS: Position = Position::new(1, 20);

    const fn rgb(r: u8, g: u8, b: u8) -> RgbColor {
        RgbColor { r, g, b }
    }

    fn themed(background: RgbColor, white: RgbColor, hover: Option<Position>) -> Terminal<TestBackend> {
        let mut theme = HostTheme { background: Some(background), ..HostTheme::default() };
        theme.palette[7] = Some(white);
        let v =
            View { light: theme.is_light() == Some(true), muted: muted(&theme), soft: soft(&theme), ..with_bar(hover) };
        render_sized(&v, W, 24)
    }

    #[rstest]
    #[case::tokyo_night(rgb(0x1a, 0x1b, 0x26), rgb(0xa9, 0xb1, 0xd6), Color::Gray)]
    #[case::github_light(rgb(0xff, 0xff, 0xff), rgb(0x6e, 0x77, 0x81), Color::Gray)]
    #[case::solarized_light(rgb(0xfd, 0xf6, 0xe3), rgb(0xee, 0xe8, 0xd5), Color::Indexed(240))]
    #[case::white_as_the_background(rgb(0x28, 0x2a, 0x36), rgb(0x28, 0x2a, 0x36), Color::Indexed(250))]
    fn icons_show_on_the_background_at_rest_and_on_hover(
        #[case] background: RgbColor,
        #[case] white: RgbColor,
        #[case] rest: Color,
    ) {
        let still = themed(background, white, None);
        let lit = themed(background, white, Some(SETTINGS));
        let (icon, name) = (&lit.backend().buffer()[SETTINGS], &lit.backend().buffer()[(WIDTH + 1, SETTINGS.y)]);
        let bold = |cell: &ratatui::buffer::Cell| cell.modifier.contains(Modifier::BOLD);
        assert_eq!(
            (still.backend().buffer()[SETTINGS].fg, icon.fg, bold(icon), name.fg, bold(name)),
            (rest, Color::Reset, true, Color::Reset, true)
        );
    }
}
