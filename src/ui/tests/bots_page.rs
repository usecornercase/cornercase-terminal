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

fn shown(sidebar: Sidebar, entries: Vec<BotEntry>, page: bool) -> View<'static> {
    View {
        has_project: true,
        sidebar,
        spin: Some(0),
        bots: Some(BotsView { entries, scroll: 0, page }),
        ..view(&["cornercase", "website", "api"])
    }
}

fn offered(page: bool) -> BotsShape {
    BotsShape { offered: true, marks: 0, page }
}

fn with_bots(sidebar: Sidebar, bots: BotsShape, agents: bool) -> Areas {
    full_layout(TALL, Widths::default(), Screen { sidebar, agents, bots, ..Screen::default() })
}

fn text_of(t: &Terminal<TestBackend>, height: u16) -> String {
    (0..height).map(|y| row_text(t, Rect::new(0, y, t.backend().buffer().area.width, 1))).collect::<Vec<_>>().join("\n")
}

mod the_button {
    use super::*;

    #[rstest]
    #[case::side_by_side(Sidebar::SideBySide)]
    #[case::projects_on_top(Sidebar::ProjectsOnTop)]
    #[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
    #[case::tree(Sidebar::Tree)]
    fn sits_at_the_right_of_the_projects_title_and_moves_nothing(#[case] sidebar: Sidebar) {
        let before = with_bots(sidebar, BotsShape::default(), true);
        let a = with_bots(sidebar, offered(false), true);
        assert_eq!((a.bots_button.y, a.bots_button.right()), (a.title.y, a.title.right()));
        assert_eq!(Areas { bots_button: Rect::default(), ..a }, before);
    }

    #[test]
    fn is_not_there_when_the_window_offers_no_bots() {
        assert!(with_bots(Sidebar::SideBySide, BotsShape::default(), false).bots_button.is_empty());
    }

    #[test]
    fn grows_by_its_marks() {
        let plain = with_bots(Sidebar::SideBySide, offered(false), false).bots_button;
        let marked = with_bots(Sidebar::SideBySide, BotsShape { marks: 5, ..offered(false) }, false).bots_button;
        assert_eq!((marked.width, marked.right()), (plain.width + 5, plain.right()));
    }

    #[test]
    fn keeps_the_spinner_and_the_unread_count_of_every_bot() {
        let t = render_sized(&shown(Sidebar::SideBySide, entries(), false), W, TALL.height);
        let a = with_bots(Sidebar::SideBySide, BotsShape { marks: marks_width(true, 2), ..offered(false) }, false);
        assert_eq!(row_text(&t, a.bots_button), " ⠋ ●2 Bots › ");
    }

    fn title_row(t: &Terminal<TestBackend>, a: &Areas) -> String {
        row_text(t, middle(a.title))
    }

    #[test]
    fn drops_its_marks_before_they_cover_the_projects_title() {
        let widths = Widths { projects: 22, ..Widths::default() };
        let screen = Screen { bots: BotsShape { marks: marks_width(true, 2), ..offered(false) }, ..Screen::default() };
        let a = full_layout(TALL, widths, screen);
        assert_eq!(a.bots_button.width, 8);
        let t = render_sized(&View { widths, ..shown(Sidebar::SideBySide, entries(), false) }, W, TALL.height);
        assert_eq!(title_row(&t, &a), " Projects     Bots › ");
        assert_eq!(row_text(&t, a.bots_button), " Bots › ");
    }

    #[test]
    fn drops_its_marks_in_a_narrow_compact_menu_too() {
        let narrow = Rect { width: 36, ..SMALL };
        let screen = Screen {
            agents: true,
            bots: BotsShape { marks: marks_width(true, 2), ..offered(false) },
            ..Screen::default()
        };
        let a = full_layout(narrow, Widths::default(), screen).shown(Some(Nav::Projects));
        assert!(a.bots_button.x >= a.title.x + 10 && a.bots_button.width == 10, "{:?}", a.bots_button);
        let agents = Some(AgentsView { entries: Vec::new(), scroll: 0 });
        let v = View { nav: Some(Nav::Projects), agents, ..shown(Sidebar::SideBySide, entries(), false) };
        let text = title_row(&render_sized(&v, narrow.width, narrow.height), &a);
        assert!(text.contains(" Projects") && text.contains("Bots ›") && !text.contains('●'), "{text}");
    }

    #[test]
    fn says_9_plus_past_nine_unread() {
        assert_eq!(marks_width(false, 12), 4);
    }

    #[test]
    fn is_drawn_in_the_projects_title() {
        let quiet = entries().into_iter().map(|entry| BotEntry { unread: 0, thinking: false, ..entry }).collect();
        insta::assert_snapshot!(render_sized(&shown(Sidebar::SideBySide, quiet, false), W, TALL.height).backend());
    }
}

mod the_page {
    use super::*;

    #[rstest]
    #[case::side_by_side(Sidebar::SideBySide)]
    #[case::projects_on_top(Sidebar::ProjectsOnTop)]
    #[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
    #[case::tree(Sidebar::Tree)]
    fn takes_the_projects_list_and_the_agents_section_and_keeps_the_pane(#[case] sidebar: Sidebar) {
        let before = with_bots(sidebar, offered(false), false);
        let a = with_bots(sidebar, offered(true), true);
        assert_eq!((a.bots_title, a.bots_list), (before.title, before.list));
        assert!(a.title.is_empty() && a.list.is_empty() && a.sidebar.is_empty() && a.bots_button.is_empty());
        assert!(a.agents.is_empty() && a.agents_border.is_empty());
        assert_eq!((a.pane, a.workspaces_list), (before.pane, before.workspaces_list));
    }

    #[test]
    fn puts_projects_back_at_the_left_of_its_title() {
        let a = with_bots(Sidebar::SideBySide, offered(true), false);
        assert_eq!((a.back.x, a.back.y), (a.bots_title.x, a.bots_title.y));
    }

    #[rstest]
    #[case::side_by_side(Sidebar::SideBySide, "side_by_side")]
    #[case::projects_on_top(Sidebar::ProjectsOnTop, "projects_on_top")]
    #[case::tree(Sidebar::Tree, "tree")]
    fn lists_the_bots_and_new(#[case] sidebar: Sidebar, #[case] name: &str) {
        let drawn = render_sized(&shown(sidebar, entries(), true), W, TALL.height);
        insta::assert_snapshot!(format!("lists_the_bots_and_new_{name}"), drawn.backend());
    }

    #[test]
    fn has_new_under_the_last_bot() {
        let a = with_bots(Sidebar::SideBySide, offered(true), false);
        let new = bots_new(a.bots_list, 1, 3, 0);
        let t = render_sized(&shown(Sidebar::SideBySide, entries(), true), W, TALL.height);
        assert_eq!(new.y, bot_rows(a.bots_list, 1, 3, 0).item(2).bottom() + 1);
        assert!(row_text(&t, new).contains("+ New"));
    }

    #[test]
    fn a_click_on_a_row_hits_that_bot() {
        let a = with_bots(Sidebar::SideBySide, offered(true), false);
        let row = bot_rows(a.bots_list, 1, 3, 0).item(1);
        assert_eq!(bot_rows(a.bots_list, 1, 3, 0).at(Position::new(row.x + 4, row.y)), Some(1));
    }

    #[test]
    fn the_shown_bot_gets_the_rail_and_the_surface() {
        let mut list = entries();
        list[0].active = true;
        let a = with_bots(Sidebar::SideBySide, offered(true), false);
        let row = bot_rows(a.bots_list, 1, 3, 0).item(0);
        let t = render_sized(&shown(Sidebar::SideBySide, list, true), W, TALL.height);
        let cell = &t.backend().buffer()[(row.x, row.y)];
        assert_eq!((cell.symbol(), cell.fg, cell.bg), ("▌", Color::Cyan, DARK_SURFACE));
    }

    #[test]
    fn a_hovered_row_shows_its_menu_button_and_no_close_button() {
        let a = with_bots(Sidebar::SideBySide, offered(true), false);
        let row = bot_rows(a.bots_list, 1, 3, 0).item(2);
        let v = View { hover: Some(Position::new(row.x + 4, row.y)), ..shown(Sidebar::SideBySide, entries(), true) };
        let text = row_text(&render_sized(&v, W, TALL.height), row);
        assert!(text.contains('⋯') && !text.contains('×'), "{text}");
    }

    #[test]
    fn a_narrow_row_drops_the_details_first() {
        let widths = Widths { projects: 22, ..Widths::default() };
        let screen = Screen { sidebar: Sidebar::SideBySide, bots: offered(true), ..Screen::default() };
        let a = full_layout(TALL, widths, screen);
        let v = View { widths, ..shown(Sidebar::SideBySide, entries(), true) };
        let row = bot_rows(a.bots_list, 1, 3, 0).item(2);
        let text = row_text(&render_sized(&v, W, TALL.height), row);
        assert!(text.contains("inbox") && !text.contains("claude"), "{text}");
    }

    #[test]
    fn draws_neither_the_projects_nor_the_workspaces() {
        let t = render_sized(&shown(Sidebar::SideBySide, entries(), true), W, TALL.height);
        let text = text_of(&t, TALL.height);
        assert!(!text.contains("website") && !text.contains("Workspaces"), "{text}");
    }
}

mod the_lobby {
    use super::*;

    fn lobby_pane() -> Rect {
        with_bots(Sidebar::SideBySide, offered(true), false).pane
    }

    #[test]
    fn with_no_bot_says_what_a_bot_is_and_offers_new_and_ideas() {
        insta::assert_snapshot!(render_sized(&shown(Sidebar::SideBySide, Vec::new(), true), W, TALL.height).backend());
    }

    #[test]
    fn with_bots_asks_to_pick_one() {
        let t = render_sized(&shown(Sidebar::SideBySide, entries(), true), W, TALL.height);
        let text = text_of(&t, TALL.height);
        assert!(text.contains("Your bots") && text.contains("Pick a bot") && !text.contains("Start from"), "{text}");
    }

    #[test]
    fn a_press_on_new_or_an_idea_hits_it() {
        let t = render_sized(&shown(Sidebar::SideBySide, Vec::new(), true), W, TALL.height);
        let text = text_of(&t, TALL.height);
        let find = |needle: &str| {
            let (y, row) = text.lines().enumerate().find(|(_, row)| row.contains(needle)).expect("drawn");
            let x = row[..row.find(needle).expect("found")].chars().count();
            Position::new(u16::try_from(x).expect("x") + 1, u16::try_from(y).expect("y"))
        };
        let pane = lobby_pane();
        assert_eq!(lobby_hit(Lobby::Empty, pane, find(NEW_BOT)), Some(LobbyHit::New));
        assert_eq!(lobby_hit(Lobby::Empty, pane, find("News")), Some(LobbyHit::Idea(1)));
        assert_eq!(lobby_hit(Lobby::Empty, pane, find("Every morning")), Some(LobbyHit::Idea(1)));
        assert_eq!(lobby_hit(Lobby::Empty, pane, find("Meet your bots")), None);
    }

    #[test]
    fn a_hovered_idea_is_filled() {
        let a = with_bots(Sidebar::SideBySide, offered(true), false);
        let t = render_sized(&shown(Sidebar::SideBySide, Vec::new(), true), W, TALL.height);
        let text = text_of(&t, TALL.height);
        let y = text.lines().position(|row| row.contains("Releases")).expect("drawn");
        let at = Position::new(a.pane.x + a.pane.width / 2, u16::try_from(y).expect("y"));
        let v = View { hover: Some(at), ..shown(Sidebar::SideBySide, Vec::new(), true) };
        let t = render_sized(&v, W, TALL.height);
        assert_eq!(t.backend().buffer()[at].bg, DARK_HOVER);
    }

    #[test]
    fn leaves_the_workspaces_column_blank_and_hides_the_chat() {
        let t = render_sized(&shown(Sidebar::SideBySide, entries(), true), W, TALL.height);
        assert!(!text_of(&t, TALL.height).contains("Message"));
    }
}

mod compact {
    use super::*;

    fn compact_view(nav: Option<Nav>, page: bool) -> View<'static> {
        View { nav, ..shown(Sidebar::SideBySide, entries(), page) }
    }

    fn compact_areas(nav: Option<Nav>, agents: bool) -> Areas {
        full_layout(SMALL, Widths::default(), Screen { bots: offered(false), agents, ..Screen::default() }).shown(nav)
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
    fn no_offer_leaves_the_projects_menu_as_it_was() {
        let plain = full_layout(SMALL, Widths::default(), Screen::default()).shown(Some(Nav::Projects));
        assert!(plain.bots_button.is_empty());
    }

    #[test]
    fn the_projects_menu_shows_the_bots_button_with_its_marks() {
        insta::assert_snapshot!(render_sized(&compact_view(Some(Nav::Projects), false), 80, 30).backend());
    }

    #[test]
    fn the_bots_page_is_drawn() {
        insta::assert_snapshot!(render_sized(&compact_view(Some(Nav::Bots), true), 80, 30).backend());
    }

    #[test]
    fn the_lobby_fills_the_screen_and_the_breadcrumb_says_bots() {
        let v = View { nav: None, ..shown(Sidebar::SideBySide, Vec::new(), true) };
        let text = text_of(&render_sized(&v, 80, 30), 30);
        assert!(text.contains("Meet your bots") && text.contains(" Bots "), "{text}");
    }
}

mod rows {
    use super::*;

    #[test]
    fn a_row_too_narrow_for_the_next_run_draws_agent_and_model_instead() {
        let long =
            BotEntry { detail: "next 16 Nov 09:00".into(), fallback: "claude".into(), ..entry("triage-bot-x", "") };
        let short = BotEntry { detail: "next Mon 09:00".into(), fallback: "claude".into(), ..entry("prs", "") };
        let t = render_sized(&shown(Sidebar::SideBySide, vec![long, short], true), W, TALL.height);
        let text = text_of(&t, TALL.height);
        assert!(text.contains("claude"), "{text}");
        assert!(text.contains("next Mon 09:00"), "{text}");
        assert!(!text.contains("next 16 Nov"), "{text}");
    }

    fn row_with(t: &Terminal<TestBackend>, needle: &str) -> Option<(String, Position)> {
        (0..TALL.height).find_map(|y| {
            let text = row_text(t, Rect::new(0, y, W, 1));
            let x = u16::try_from(text[..text.find(needle)?].chars().count()).ok()?;
            Some((text, Position::new(x, y)))
        })
    }

    fn sidebar_row(t: &Terminal<TestBackend>, name: &str) -> String {
        let projects = usize::from(Widths::default().projects);
        let (text, _) = row_with(t, name).expect("the bot's row");
        text.chars().take(projects).collect()
    }

    #[test]
    fn a_bot_that_asks_for_a_secret_says_so_in_the_waiting_colour() {
        let asking = BotEntry { asking: true, fallback: "claude".into(), ..entry("campus", "asks for a secret") };
        let wide = Widths { projects: 44, ..Widths::default() };
        let view = View { widths: wide, ..shown(Sidebar::SideBySide, vec![asking, entry("prs", "claude")], true) };
        let t = render_sized(&view, W, TALL.height);
        let (_, at) = row_with(&t, "asks").expect("the detail is drawn");
        assert_eq!(t.backend().buffer()[at].fg, WAITING_COLOR);
        let plain = (0..TALL.height).find_map(|y| {
            let text = row_text(&t, Rect::new(0, y, W, 1));
            let x = text.rfind("claude")?;
            Some(Position::new(u16::try_from(text[..x].chars().count()).ok()?, y))
        });
        assert_ne!(t.backend().buffer()[plain.expect("the other detail")].fg, WAITING_COLOR);
    }

    #[test]
    fn a_row_too_narrow_for_the_whole_ask_says_asks_in_the_waiting_colour() {
        let asking =
            BotEntry { asking: true, unread: 1, fallback: "claude".into(), ..entry("campus", "asks for a secret") };
        let t = render_sized(&shown(Sidebar::SideBySide, vec![asking], true), W, TALL.height);
        let row = sidebar_row(&t, "campus");
        assert!(row.contains("asks"), "{row}");
        assert!(!row.contains("for a secret"), "{row}");
        assert!(!row.contains("claude"), "{row}");
        let (_, at) = row_with(&t, "asks").expect("asks is drawn");
        assert_eq!(t.backend().buffer()[at].fg, WAITING_COLOR);
    }

    #[test]
    fn an_asking_row_cuts_a_long_name_so_asks_still_fits() {
        let name = "a-bot-with-a-very-long-name-of-forty-chr";
        let asking =
            BotEntry { asking: true, unread: 1, fallback: "claude".into(), ..entry(name, "asks for a secret") };
        let plain = BotEntry { asking: false, ..asking.clone() };
        let drawn = |entry: BotEntry| {
            let t = render_sized(&shown(Sidebar::SideBySide, vec![entry], true), W, TALL.height);
            sidebar_row(&t, "a-bot")
        };
        let (asked, quiet) = (drawn(asking), drawn(plain));
        assert!(asked.contains('…'), "{asked}");
        assert!(asked.contains("asks"), "{asked}");
        let shown_of = |row: &str| row.split('…').next().map_or(0, |kept| kept.trim_end().chars().count());
        assert!(shown_of(&quiet) > shown_of(&asked), "{quiet}\n{asked}");
    }

    #[test]
    fn an_asking_row_cuts_a_wide_name_by_its_cells_so_asks_does_not_cover_it() {
        let name = "日本語の名前のとても長いボット";
        let asking =
            BotEntry { asking: true, unread: 1, fallback: "claude".into(), ..entry(name, "asks for a secret") };
        let t = render_sized(&shown(Sidebar::SideBySide, vec![asking], true), W, TALL.height);
        let (_, at) = row_with(&t, "asks").expect("asks is drawn");
        let buffer = t.backend().buffer();
        assert_eq!(buffer[at].fg, WAITING_COLOR);
        assert_eq!(buffer[Position::new(at.x - 1, at.y)].symbol(), " ");
        let row = sidebar_row(&t, "日");
        assert!(row.contains('…'), "{row}");
    }
}
