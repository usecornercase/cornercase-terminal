use std::time::Instant;

use super::*;
use crate::project::search::Finder;
use crate::settings::Page;

const PREFIX_KEY: &str = "UI › prefix key";

fn searching(query: &str) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    app.projects[0].name = Some("prefix-lab".into());
    click(&mut app, areas().search.as_position());
    type_text(&mut app, query);
    (app, rx, dirs)
}

fn shown(app: &App) -> Vec<ui::ResultRow> {
    let Some(Overlay::Search(dialog)) = &app.overlay else { panic!("the search is open") };
    let ui::Overlay::Search(search) = app.search_view(dialog) else { panic!("the search draws") };
    search.results
}

fn names(app: &App) -> Vec<String> {
    shown(app).into_iter().map(|row| row.name).collect()
}

fn position_of(app: &App, name: &str) -> usize {
    names(app).iter().position(|row| row == name).expect("the setting is listed")
}

fn click_result(app: &mut App, i: usize) {
    let pos = ui::result_item(areas().results, names(app).len(), 0, i, 1).as_position();
    click(app, pos);
}

fn settings(app: &App) -> &Settings {
    match &app.overlay {
        Some(Overlay::Settings(settings)) => settings,
        _ => panic!("settings are open"),
    }
}

fn chosen_row(app: &App) -> String {
    let settings = settings(app);
    settings.row_text(&settings.rows()[settings.cursor]).label
}

fn down_to(app: &mut App, i: usize) {
    for _ in 0..i {
        send_key(app, KeyCode::Down, KeyModifiers::NONE);
    }
}

mod the_list {
    use super::*;

    #[test]
    fn a_setting_comes_after_the_names_and_says_it_is_one() {
        let (app, _rx, _dirs) = searching("prefix");

        let rows = shown(&app);

        assert_eq!(rows[0].name, "prefix-lab");
        let setting = rows.iter().find(|row| row.name == PREFIX_KEY).expect("the setting is listed");
        assert_eq!(setting.context, "off · settings");
    }

    #[test]
    fn the_names_keep_their_positions() {
        let (app, _rx, _dirs) = searching("prefix");

        let places = app.search_results("prefix");

        assert_eq!(places[0].name, "prefix-lab");
        let listed = names(&app);
        assert!(places.iter().zip(&listed).all(|(place, name)| place.name == *name));
    }

    #[rstest::rstest]
    #[case::every_word("prefix key", true)]
    #[case::any_order("key prefix", true)]
    #[case::the_start_of_a_word("pref", true)]
    #[case::one_word_missing("prefix zebra", false)]
    #[case::the_middle_of_a_word("refix", false)]
    fn a_word_matches_from_its_start_and_every_word_must(#[case] query: &str, #[case] listed: bool) {
        let (app, _rx, _dirs) = searching(query);

        assert_eq!(names(&app).iter().any(|name| name == PREFIX_KEY), listed);
    }

    #[test]
    fn an_empty_query_lists_no_settings() {
        let (mut app, _rx, _dirs) = searching("  ");

        assert!(shown(&app).is_empty());
        app.refresh(Instant::now());
        assert!(shown(&app).is_empty());
    }

    #[test]
    fn the_search_in_what_was_said_lists_no_settings() {
        let (mut app, _rx, _dirs) = searching("prefix");

        send_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);

        assert!(!names(&app).iter().any(|name| name == PREFIX_KEY));
    }

    #[test]
    fn going_back_to_names_lists_them_again() {
        let (mut app, _rx, _dirs) = searching("prefix");

        send_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        send_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);

        assert!(names(&app).iter().any(|name| name == PREFIX_KEY));
    }

    #[test]
    fn a_hidden_row_is_muted_with_its_reason() {
        let (app, _rx, _dirs) = searching("closed workspaces");

        let rows = shown(&app);
        let hidden = rows.iter().find(|row| row.muted).expect("a hidden row");

        assert_eq!(hidden.context, "shows once forum mode is on · settings");
    }

    #[test]
    fn a_setting_that_shows_is_not_muted() {
        let (app, _rx, _dirs) = searching("prefix key");

        assert!(shown(&app).iter().all(|row| !row.muted));
    }

    #[test]
    fn every_setting_found_can_be_scrolled_to() {
        let (mut app, _rx, _dirs) = searching("a");
        let last = names(&app).len() - 1;
        let rows = usize::from(ui::results_list(areas().results).height);
        assert!(last >= rows, "more than a screen of results");

        down_to(&mut app, last);

        let Some(Overlay::Search(dialog)) = &app.overlay else { panic!("the search is open") };
        assert_eq!(dialog.field.selected(), last);
        assert!(dialog.field.scroll() > 0);
    }
}

mod choosing {
    use super::*;

    #[test]
    fn enter_opens_settings_on_the_row() {
        let (mut app, _rx, _dirs) = searching("prefix key");

        send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!((settings(&app).page, chosen_row(&app).as_str()), (Page::Ui, "prefix key"));
        assert_eq!(app.settings_page, Page::Ui);
    }

    #[test]
    fn a_click_opens_settings_on_the_row() {
        let (mut app, _rx, _dirs) = searching("prefix key");

        let i = position_of(&app, PREFIX_KEY);
        click_result(&mut app, i);

        assert_eq!((settings(&app).page, chosen_row(&app).as_str()), (Page::Ui, "prefix key"));
    }

    #[test]
    fn a_click_on_a_name_still_goes_there() {
        let (mut app, _rx, _dirs) = searching("prefix");

        click_result(&mut app, 0);

        assert!(app.overlay.is_none());
    }

    #[test]
    fn it_changes_and_saves_nothing() {
        let (dir, config) = (TempDir::new(), TempDir::new());
        let file = config.path().join("config.json");
        let (mut app, _rx) = app_in(dir.path(), file.clone());
        let before = app.config.clone();
        click(&mut app, areas().search.as_position());
        type_text(&mut app, "prefix key");

        send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(app.config, before);
        assert!(!file.exists());
    }

    #[test]
    fn a_hidden_row_lands_on_its_parent_with_a_notice() {
        let (mut app, _rx, _dirs) = searching("closed workspaces");
        let i = shown(&app).iter().position(|row| row.muted).expect("a hidden row");

        down_to(&mut app, i);
        send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(settings(&app).page, Page::Remote);
        assert!(settings(&app).notice.as_deref().is_some_and(|notice| notice.contains("shows once")));
    }

    #[test]
    fn the_flows_tab_opens_as_the_tab_it_is() {
        let (mut app, _rx, _dirs) = searching("flows");
        let i = position_of(&app, "Flows");

        down_to(&mut app, i);
        send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        assert!(matches!(app.overlay, Some(Overlay::Flows { .. })));
    }

    #[test]
    fn a_new_query_asks_again() {
        let (mut app, _rx, _dirs) = searching("prefix");
        let Some(Overlay::Search(Finder { settings: Some((asked, _)), .. })) = &app.overlay else {
            panic!("the settings were asked");
        };
        assert_eq!(asked, "prefix");

        type_text(&mut app, " key");

        let Some(Overlay::Search(Finder { settings: Some((asked, _)), .. })) = &app.overlay else {
            panic!("the settings were asked");
        };
        assert_eq!(asked, "prefix key");
    }
}
