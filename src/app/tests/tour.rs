use super::*;
use crate::tour::{self, CORE, Saved, TOURS};
use crate::ui::TourButton;

struct Tour {
    app: App,
    rx: Receiver<AppEvent>,
    dir: TempDir,
    _projects: Vec<TempDir>,
}

impl Tour {
    fn with(projects: usize, first_run: bool, file: Option<&Saved>) -> Self {
        let (mut app, rx, dirs) = app_with(projects);
        let dir = TempDir::new();
        let path = dir.path().join("tour.json");
        if let Some(saved) = file {
            tour::save(&path, saved).expect("saved");
        }
        app.set_tour(path, first_run);
        Self { app, rx, dir, _projects: dirs }
    }

    fn first_run(projects: usize) -> Self {
        let mut t = Self::with(projects, true, None);
        t.offer();
        t
    }

    fn offer(&mut self) {
        render(&mut self.app, AREA);
        self.app.offer_tour(Instant::now());
        render(&mut self.app, AREA);
    }

    fn step(&self) -> Option<(&'static str, &'static str)> {
        let active = self.app.tour.active.as_ref().filter(|a| a.card)?;
        let tour = &TOURS[active.tour];
        Some((tour.id, tour.steps[active.step].id))
    }

    fn saved(&self) -> Saved {
        tour::load(&self.dir.path().join("tour.json")).expect("a file")
    }

    fn card(&self) -> ui::TourCard {
        self.app.tour_card(AREA).expect("a card")
    }

    fn button(&self, button: TourButton) -> Position {
        let layout = ui::tour_layout(AREA, &self.card());
        let (_, rect) = layout.buttons.into_iter().find(|(b, _)| *b == button).expect("the button");
        rect.as_position()
    }

    fn press(&mut self, button: TourButton) {
        let at = self.button(button);
        click(&mut self.app, at);
        self.app.offer_tour(Instant::now());
    }
}

mod when_it_shows {
    use super::*;

    #[test]
    fn a_new_install_starts_the_core_tour_on_the_first_window() {
        let t = Tour::first_run(0);
        assert_eq!(t.step(), Some((CORE, "projects")));
        assert!(t.saved().offered);
    }

    #[test]
    fn without_a_window_it_waits() {
        let mut t = Tour::with(0, true, None);
        t.app.offer_tour(Instant::now());
        assert_eq!(t.step(), None);
        t.offer();
        assert_eq!(t.step(), Some((CORE, "projects")));
    }

    #[test]
    fn an_install_from_before_gets_one_toast_and_no_tour() {
        let mut t = Tour::with(1, false, None);
        t.offer();
        assert_eq!(t.step(), None);
        assert_eq!(toast(&t.app), Some(tour_card::ANNOUNCE));
        assert!(t.saved().announced);
        t.app.toast = None;
        t.offer();
        assert_eq!(toast(&t.app), None);
    }

    #[test]
    fn an_install_with_a_tour_file_hears_nothing() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        t.offer();
        assert_eq!((t.step(), toast(&t.app)), (None, None));
    }

    #[test]
    fn turned_off_it_never_shows_by_itself() {
        let mut t = Tour::with(0, true, None);
        t.app.config.tour = false;
        t.offer();
        assert_eq!(t.step(), None);
        assert!(t.saved().offered);
    }

    #[test]
    fn it_is_offered_once() {
        let mut t = Tour::first_run(0);
        t.press(TourButton::End);
        t.offer();
        assert_eq!(t.step(), None);
    }
}

mod completing {
    use super::*;

    #[test]
    fn opening_a_project_completes_the_first_step() {
        let mut t = Tour::first_run(0);
        let dir = TempDir::new();
        t.app.open_project(dir.path().to_path_buf(), AREA).expect("opened");
        t.app.offer_tour(Instant::now());
        assert_eq!(t.step(), Some((CORE, "workspaces")));
        assert_eq!(t.saved().progress(CORE).done, ["projects"]);
    }

    #[test]
    fn a_new_tab_completes_the_tabs_step() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        t.app.start_tour(0, Some(2), true);
        t.app.add_tab(0, 0, AREA).expect("a tab");
        t.app.offer_tour(Instant::now());
        assert_eq!(t.step(), Some((CORE, "prompt")));
    }

    #[test]
    fn a_step_that_only_explains_waits_for_next() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        t.app.start_tour(0, Some(4), true);
        t.app.offer_tour(Instant::now());
        assert_eq!(t.step(), Some((CORE, "status")));
        assert_eq!(t.card().forward, TourButton::Next);
        t.press(TourButton::Next);
        assert_eq!(t.step(), Some((CORE, "settings")));
        assert_eq!(t.saved().progress(CORE).done, ["status"]);
    }

    #[test]
    fn a_project_already_open_turns_the_first_step_into_reading() {
        let mut t = Tour::first_run(1);
        assert_eq!(t.card().forward, TourButton::Next);
        t.press(TourButton::Next);
        assert_eq!(t.step(), Some((CORE, "workspaces")));
    }

    #[test]
    fn skip_moves_on_without_marking_and_back_returns() {
        let mut t = Tour::first_run(0);
        assert_eq!(t.card().forward, TourButton::Skip);
        t.press(TourButton::Skip);
        assert_eq!(t.step(), Some((CORE, "workspaces")));
        t.press(TourButton::Back);
        assert_eq!(t.step(), Some((CORE, "projects")));
        assert!(t.saved().progress(CORE).done.is_empty());
    }

    #[test]
    fn the_last_step_finishes_the_tour() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        t.app.start_tour(0, Some(6), true);
        assert_eq!(t.card().forward, TourButton::Done);
        t.press(TourButton::Done);
        assert_eq!(t.step(), None);
        assert!(t.saved().finished(CORE));
    }

    #[test]
    fn turning_on_the_agents_section_is_the_only_setting_it_changes() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        t.app.config_path = t.dir.path().join("config.json");
        t.app.start_tour(0, Some(4), true);
        assert!(!t.app.config.agents_section);
        t.press(TourButton::TurnOn);
        assert!(t.app.config.agents_section);
    }

    #[test]
    fn opening_the_todo_panel_completes_its_tour_step() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        let todo = TOURS.iter().position(|t| t.id == "todo").expect("todo");
        t.app.start_tour(todo, None, true);
        t.app.toggle_todo();
        t.app.offer_tour(Instant::now());
        assert_eq!(t.step(), Some(("todo", "add")));
    }
}

mod around_the_card {
    use super::*;

    #[test]
    fn keys_reach_the_pane_while_it_shows() {
        let mut t = Tour::first_run(1);
        let rx = std::mem::replace(&mut t.rx, mpsc::channel().1);
        type_in_pane(&mut t.app, &rx, "tour");
        assert!(t.step().is_some());
    }

    #[test]
    fn a_click_outside_it_reaches_the_window() {
        let mut t = Tour::first_run(1);
        click(&mut t.app, areas().search.as_position());
        assert!(matches!(t.app.overlay, Some(Overlay::Search(_))));
        assert!(t.step().is_some());
    }

    #[test]
    fn a_press_on_it_never_reaches_what_is_under_it() {
        let mut t = Tour::first_run(1);
        let layout = ui::tour_layout(AREA, &t.card());
        click(&mut t.app, Position::new(layout.area.x + 1, layout.area.y + 1));
        assert_eq!(t.app.term().expect("a pane").input_at, None);
        assert!(t.app.overlay.is_none());
    }

    #[test]
    fn esc_closes_a_dialog_and_keeps_the_tour() {
        let mut t = Tour::first_run(1);
        t.app.open_settings();
        send_key(&mut t.app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(t.app.overlay.is_none());
        assert!(t.step().is_some());
    }

    #[test]
    fn it_is_drawn_over_dialogs_and_points_at_its_target() {
        let mut t = Tour::first_run(0);
        let card = t.card();
        assert!(!card.target.is_empty() && card.button);
        let terminal = render(&mut t.app, AREA);
        let buf = terminal.backend().buffer();
        assert_eq!(buf[(card.target.x, card.target.y)].bg, ratatui::style::Color::Cyan);
        let layout = ui::tour_layout(AREA, &card);
        assert!(layout.area.intersection(card.target).is_empty());
    }

    #[test]
    fn docs_copies_the_tours_page() {
        let mut t = Tour::first_run(1);
        t.press(TourButton::Docs);
        let url = TOURS[0].docs_url();
        assert_eq!(t.app.take_host_writes(), [clipboard::osc52(&url)]);
    }
}

mod first_prompt {
    use super::*;

    fn prompt_step() -> Tour {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        t.app.config.agent = "claude".into();
        t.app.config.agent_commands.insert("claude".into(), "/bin/cat".into());
        t.app.start_tour(0, Some(3), true);
        t
    }

    #[test]
    fn the_card_offers_an_idea_to_edit_and_the_agent_to_start() {
        let t = prompt_step();
        let card = t.card();
        assert_eq!(card.agent.as_deref(), Some("claude"));
        assert_eq!(card.actions, [TourButton::Idea, TourButton::Start]);
        assert!(card.field.as_ref().is_some_and(|f| !f.is_empty()));
    }

    #[test]
    fn typing_goes_to_the_field_once_clicked_and_back_to_the_pane_after() {
        let mut t = prompt_step();
        let field = ui::tour_layout(AREA, &t.card()).field;
        click(&mut t.app, field.as_position());
        assert!(t.app.tour_typing());
        send_key(&mut t.app, KeyCode::Char('!'), KeyModifiers::NONE);
        assert!(t.card().field.expect("a field").text().contains('!'));
        assert_eq!(t.app.term().expect("a pane").input_at, None);
        click(&mut t.app, areas().pane.as_position());
        assert!(!t.app.tour_typing());
    }

    #[test]
    fn another_idea_replaces_the_text() {
        let mut t = prompt_step();
        let before = t.card().field.expect("a field").text();
        t.press(TourButton::Idea);
        assert_ne!(t.card().field.expect("a field").text(), before);
    }

    #[test]
    fn start_sends_a_start_request_in_this_workspace() {
        let mut t = prompt_step();
        let tabs = t.app.projects.tabs().count();
        t.press(TourButton::Start);
        assert_eq!(t.app.projects.tabs().count(), tabs + 1);
        assert_eq!(t.step(), Some((CORE, "prompt")));
        assert!(t.card().note.is_some());
    }

    #[test]
    fn without_an_agent_it_says_how_to_get_one() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        t.app.config.agent_commands = ["claude", "codex", "gemini", "opencode", "grok", "cursor", "copilot"]
            .into_iter()
            .map(|k| (k.to_string(), "/nonexistent/agent".to_string()))
            .collect();
        t.app.config.agent = "auto".into();
        t.app.start_tour(0, Some(3), true);
        let card = t.card();
        if card.agent.is_none() {
            assert!(card.actions.is_empty());
            assert!(card.note.is_some_and(|n| n.contains("No agent")));
        }
    }
}

mod help {
    use super::*;

    fn help_at(app: &App) -> Position {
        ui::help_button(app_areas(app).quit).as_position()
    }

    fn menu(app: &App) -> Vec<String> {
        match &app.overlay {
            Some(Overlay::Menu { actions, .. }) => app.menu_labels(actions),
            _ => Vec::new(),
        }
    }

    fn pick(app: &mut App, label: &str) {
        let Some(Overlay::Menu { at, actions }) = &app.overlay else { panic!("no menu") };
        let labels = app.menu_labels(actions);
        let menu = ui::menu_area(AREA, *at, &labels);
        let i = labels.iter().position(|l| l == label).unwrap_or_else(|| panic!("no {label} in {labels:?}"));
        click(app, ui::menu_item(menu, i).as_position());
    }

    #[test]
    fn the_button_sits_on_the_quit_row_and_opens_the_menu() {
        let (mut app, _rx) = app();
        render(&mut app, AREA);
        let help = ui::help_button(app_areas(&app).quit);
        assert!(!help.is_empty());
        click(&mut app, help.as_position());
        assert_eq!(menu(&app), ["tour", "catalogue…", "docs", "report a bug"]);
        assert!(!app.outbox.detach);
    }

    #[test]
    fn quit_still_quits_beside_it() {
        let (mut app, _rx) = app();
        render(&mut app, AREA);
        let quit = app_areas(&app).quit.as_position();
        click(&mut app, quit);
        assert!(app.outbox.detach);
    }

    #[test]
    fn the_keys_menu_shows_with_a_prefix_key() {
        let (mut app, _rx) = app();
        app.config.prefix_key = "ctrl+]".into();
        render(&mut app, AREA);
        let at = help_at(&app);
        click(&mut app, at);
        assert!(menu(&app).contains(&"keys menu".to_string()));
    }

    #[test]
    fn the_catalogue_lists_every_tour_and_starts_one() {
        let mut t = Tour::with(1, false, Some(&Saved::default()));
        render(&mut t.app, AREA);
        let at = help_at(&t.app);
        click(&mut t.app, at);
        pick(&mut t.app, "catalogue…");
        assert_eq!(menu(&t.app).len(), TOURS.len());
        pick(&mut t.app, "TODO");
        assert_eq!(t.step(), Some(("todo", "open")));
    }

    #[test]
    fn report_a_bug_copies_the_issue_link() {
        let (mut app, _rx) = app();
        render(&mut app, AREA);
        let at = help_at(&app);
        click(&mut app, at);
        pick(&mut app, "report a bug");
        assert_eq!(app.take_host_writes(), [clipboard::osc52(help_menu::BUG_URL)]);
    }

    #[test]
    fn h_in_the_keys_menu_opens_it() {
        let (mut app, _rx) = app();
        app.config.prefix_key = "ctrl+]".into();
        send_key(&mut app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        send_key(&mut app, KeyCode::Char('h'), KeyModifiers::NONE);
        assert_eq!(menu(&app).first().map(String::as_str), Some("tour"));
    }

    #[test]
    fn a_started_tour_continues_from_help() {
        let mut t = Tour::first_run(1);
        t.press(TourButton::Next);
        t.press(TourButton::End);
        render(&mut t.app, AREA);
        let at = help_at(&t.app);
        click(&mut t.app, at);
        pick(&mut t.app, "continue the tour");
        assert_eq!(t.step(), Some((CORE, "workspaces")));
    }

    #[test]
    fn the_icon_bar_has_it_above_usage() {
        let (mut app, _rx) = app();
        app.config.icon_bar = true;
        render(&mut app, AREA);
        let bar = app_areas(&app).activity;
        let items = ui::activity_items(bar, app.activity_shown());
        let (_, help) = items.iter().find(|(i, _)| *i == ui::ActivityItem::Help).expect("help");
        let (_, usage) = items.iter().find(|(i, _)| *i == ui::ActivityItem::Usage).expect("usage");
        assert!(help.y < usage.y);
        click(&mut app, help.as_position());
        assert_eq!(menu(&app).first().map(String::as_str), Some("tour"));
    }
}
