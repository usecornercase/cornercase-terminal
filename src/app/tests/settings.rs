use super::*;
use crate::issues::secrets;
use crate::settings::{Detail, Row};
use crate::test_util::FakeHttp;

fn issues_after_settings(s: &mut Setup) -> &Browser {
    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
    click(&mut s.app, areas().issues.as_position());
    let Some(Overlay::Issues(b)) = &s.app.overlay else { panic!("the issues are not open") };
    b
}

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    config: TempDir,
    _dir: TempDir,
}

fn open() -> Setup {
    let (dir, config) = (TempDir::new(), TempDir::new());
    let (mut app, rx) = app_in(dir.path(), config.path().join("config.json"));
    app.issues.env_tokens.clear();
    click(&mut app, areas().settings.as_position());
    Setup { app, rx, config, _dir: dir }
}

fn form(app: &App) -> &Settings {
    let Some(Overlay::Settings(s)) = &app.overlay else { panic!("the settings are not open") };
    s
}

fn config_path(s: &Setup) -> PathBuf {
    s.config.path().join("config.json")
}

fn secrets_file(s: &Setup) -> PathBuf {
    paths::secrets(&config_path(s))
}

fn sections(app: &App) -> Vec<&'static str> {
    form(app).rows().iter().map(Row::section).collect()
}

fn show(app: &mut App, page: Page) {
    let names = Page::ALL.map(Page::name);
    let i = Page::ALL.iter().position(|p| *p == page).expect("a page");
    click(app, ui::settings_tabs(ui::settings_area(AREA), &names)[i].as_position());
}

fn click_row(s: &mut Setup, row: &Row) {
    show(&mut s.app, row.page());
    let i = form(&s.app).rows().iter().position(|r| r == row).expect("the row is there");
    let pos = ui::settings_row(AREA, &sections(&s.app), form(&s.app).cursor, i).as_position();
    click(&mut s.app, pos);
}

fn enter(s: &mut Setup) {
    send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
}

fn shortcut(s: &mut Setup, status: u16) -> FakeHttp {
    fake_shortcut(&mut s.app, vec![("GET /api/v3/member", status, crate::test_util::SHORTCUT_MEMBER)])
}

fn paste_shortcut_token(s: &mut Setup, token: &str) {
    click_row(s, &Row::Token(Source::Shortcut));
    type_text(&mut s.app, token);
    enter(s);
}

fn go_to(s: &mut Setup, row: &Row) {
    show(&mut s.app, row.page());
    let i = form(&s.app).rows().iter().position(|r| r == row).expect("the row is there");
    while form(&s.app).cursor < i {
        send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
    }
}

fn pick(s: &mut Setup, row: &Row, value: &str) {
    go_to(s, row);
    enter(s);
    type_text(&mut s.app, value);
    enter(s);
}

#[test]
fn button_opens_them_with_the_current_config() {
    let s = open();
    assert_eq!(form(&s.app).config.worktrees_dir, config::DEFAULT_WORKTREES_DIR);
}

#[test]
fn the_folder_is_saved_at_once() {
    let mut s = open();
    click_row(&mut s, &Row::Folder);
    while form(&s.app).edit.as_ref().is_some_and(|e| !e.input.is_empty()) {
        send_key(&mut s.app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    type_text(&mut s.app, "/srv/worktrees");

    enter(&mut s);

    assert_eq!(
        (config::load(&config_path(&s)).worktrees_dir.as_str(), s.app.config.worktrees_dir.as_str()),
        ("/srv/worktrees", "/srv/worktrees")
    );
}

#[test]
fn a_relative_folder_is_refused() {
    let mut s = open();
    click_row(&mut s, &Row::Folder);
    type_text(&mut s.app, "x");
    form(&s.app);
    if let Some(Overlay::Settings(f)) = &mut s.app.overlay {
        f.edit.as_mut().expect("editing").input = "worktrees".into();
    }

    enter(&mut s);

    assert_eq!(
        (form(&s.app).edit.as_ref().and_then(|e| e.error.clone()).is_some(), config_path(&s).exists()),
        (true, false)
    );
}

#[test]
fn a_click_on_a_tab_shows_its_rows() {
    let mut s = open();
    show(&mut s.app, Page::Ui);
    assert_eq!(
        form(&s.app).rows(),
        [
            Row::Sidebar,
            Row::Tabs,
            Row::IconBar,
            Row::AgentsSection,
            Row::WorkingIcon,
            Row::Counts,
            Row::TodoDot,
            Row::DimPanes,
            Row::Detail(Detail::Model),
            Row::Detail(Detail::Context),
            Row::Detail(Detail::Memory),
            Row::Notifications,
            Row::Sound,
            Row::SoundWaiting,
            Row::SoundDone,
            Row::SoundSubagents,
            Row::Updates,
            Row::StarPrompt,
            Row::Prefix
        ]
    );
}

#[test]
fn they_open_again_on_the_last_tab() {
    let mut s = open();
    send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

    click(&mut s.app, areas().settings.as_position());

    assert_eq!(form(&s.app).page, Page::Agents);
}

#[test]
fn esc_closes_them() {
    let mut s = open();
    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(s.app.overlay.is_none());
}

#[test]
fn a_stacked_sidebar_applies_at_once() {
    let mut s = open();
    click_row(&mut s, &Row::Sidebar);
    type_text(&mut s.app, "projects");
    enter(&mut s);
    assert_eq!(
        (config::load(&config_path(&s)).sidebar, s.app.layout(AREA).workspaces_border),
        ("projects_on_top".to_string(), Rect::default())
    );
}

#[test]
fn a_good_token_is_checked_and_saved() {
    let mut s = open();
    let server = shortcut(&mut s, 200);

    paste_shortcut_token(&mut s, "t0k");

    pump_until(&mut s.app, &s.rx, "the check finishes", |a| !form(a).busy());
    assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token").as_deref(), Some("t0k"));
    assert!(server.request(0).to_lowercase().contains("shortcut-token: t0k"));
    assert!(matches!(form(&s.app).tokens[0].1, Status::Saved(Some(_))));
}

#[test]
fn a_rejected_token_stays_in_its_field_and_is_not_saved() {
    let mut s = open();
    let _server = shortcut(&mut s, 401);

    paste_shortcut_token(&mut s, "bad");

    pump_until(&mut s.app, &s.rx, "the check fails", |a| !form(a).busy());
    let error = form(&s.app).edit.as_ref().and_then(|e| e.error.clone());
    assert_eq!(error.as_deref(), Some("Shortcut rejected the token"));
    assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token"), None);
}

#[test]
fn a_saved_token_shows_as_connected() {
    let (dir, config) = (TempDir::new(), TempDir::new());
    secrets::write(&paths::secrets(&config.path().join("config.json")), "linear_api_key", "k").expect("save");
    let (mut app, _rx) = app_in(dir.path(), config.path().join("config.json"));
    app.issues.env_tokens.clear();

    click(&mut app, areas().settings.as_position());

    assert_eq!(form(&app).tokens[1].1, Status::Saved(None));
}

#[test]
fn the_remove_button_forgets_a_saved_token() {
    let mut s = open();
    secrets::write(&secrets_file(&s), "shortcut_token", "t0k").expect("save");
    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
    click(&mut s.app, areas().settings.as_position());
    show(&mut s.app, Page::Issues);
    let row = ui::settings_row(AREA, &sections(&s.app), 0, 0);

    click(&mut s.app, ui::settings_remove(row).as_position());

    assert_eq!(
        (secrets::read(&secrets_file(&s), "shortcut_token"), &form(&s.app).tokens[0].1),
        (None, &Status::Missing)
    );
}

#[test]
fn a_token_saved_here_opens_the_issues_tab_without_asking() {
    let mut s = open();
    let _server = shortcut(&mut s, 200);
    paste_shortcut_token(&mut s, "t0k");
    pump_until(&mut s.app, &s.rx, "the check finishes", |a| !form(a).busy());
    let b = issues_after_settings(&mut s);
    assert!(b.connections.contains_key(&Source::Shortcut));
}

#[test]
fn a_mode_picked_here_is_how_the_agent_starts() {
    let mut s = open();

    pick(&mut s, &Row::Kind("claude".into()), "plan");

    assert_eq!(agents::command_line(&config::load(&config_path(&s)), "claude"), "claude --permission-mode plan");
}

#[test]
fn the_default_agent_picked_here_takes_the_issues() {
    let mut s = open();
    pick(&mut s, &Row::DefaultAgent, "codex");
    let b = issues_after_settings(&mut s);
    assert_eq!(b.agents.default.as_deref(), Some("codex"));
}

#[test]
fn hiding_a_tab_hides_it_in_the_issues() {
    let mut s = open();
    go_to(&mut s, &Row::Tab("linear"));
    enter(&mut s);
    let b = issues_after_settings(&mut s);
    assert!(!b.tabs.contains(&IssueTab::One(Source::Linear)));
}

mod config_file {
    use super::*;

    fn later(s: &mut Setup) {
        s.app.refresh(Instant::now() + Duration::from_secs(2));
    }

    fn edit_outside(s: &Setup, change: impl FnOnce(&mut Config)) {
        let mut config = config::load(&config_path(s));
        if !config_path(s).exists() {
            config = s.app.config.clone();
        }
        change(&mut config);
        write_config(s.config.path(), &config);
    }

    #[test]
    fn an_edit_from_outside_is_applied_and_shown_in_the_open_modal() {
        let mut s = open();
        let before = s.app.config_file.revision;
        edit_outside(&s, |c| c.counts = false);

        later(&mut s);

        assert_eq!(
            (s.app.config.counts, form(&s.app).config.counts, s.app.config_file.revision > before),
            (false, false, true)
        );
    }

    fn work_profile(c: &mut Config) {
        let accounts = std::collections::BTreeMap::from([("claude".to_string(), "work".to_string())]);
        c.profiles.insert("Work".into(), accounts);
        c.profile = "Work".into();
    }

    #[test]
    fn a_profile_chosen_outside_is_applied_and_raises_the_revision() {
        let mut s = open();
        let before = s.app.config_file.revision;
        edit_outside(&s, work_profile);

        later(&mut s);

        assert_eq!(s.app.config.active_profile().map(|(name, _)| name), Some("Work"));
        assert!(s.app.config_file.revision > before);
    }

    #[test]
    fn going_back_to_the_system_outside_is_applied() {
        let mut s = open();
        edit_outside(&s, work_profile);
        later(&mut s);
        let before = s.app.config_file.revision;
        edit_outside(&s, |c| c.profile.clear());

        s.app.refresh(Instant::now() + Duration::from_secs(4));

        assert_eq!((s.app.config.profile.as_str(), s.app.config_file.revision > before), ("", true));
    }

    #[test]
    fn a_change_in_the_modal_keeps_a_profile_chosen_outside() {
        let mut s = open();
        edit_outside(&s, work_profile);

        click_row(&mut s, &Row::Detail(Detail::Memory));

        let saved = config::load(&config_path(&s));
        assert_eq!((saved.profile.as_str(), saved.memory), ("Work", true));
    }

    #[test]
    fn the_modal_reads_the_file_before_it_saves() {
        let mut s = open();
        edit_outside(&s, |c| c.counts = false);

        click_row(&mut s, &Row::Detail(Detail::Memory));

        let saved = config::load(&config_path(&s));
        assert_eq!((saved.counts, saved.memory), (false, true));
    }

    #[test]
    fn a_file_that_cannot_be_read_keeps_the_settings_and_is_not_written_over() {
        let mut s = open();
        std::fs::write(config_path(&s), "{").expect("write");

        later(&mut s);
        click_row(&mut s, &Row::Detail(Detail::Memory));

        assert_eq!(
            (
                s.app.config.memory,
                s.app.config_file.problem.is_some(),
                toast(&s.app).is_some_and(|t| t.contains("cannot be read")),
                std::fs::read_to_string(config_path(&s)).expect("read").as_str(),
            ),
            (false, true, true, "{")
        );
    }

    #[test]
    fn a_fixed_file_is_applied() {
        let mut s = open();
        std::fs::write(config_path(&s), "{").expect("write");
        later(&mut s);

        write_config(s.config.path(), &Config { counts: false, ..s.app.config.clone() });
        s.app.refresh(Instant::now() + Duration::from_secs(4));

        assert_eq!((s.app.config.counts, s.app.config_file.problem.is_none()), (false, true));
    }

    #[test]
    fn a_missing_file_keeps_the_settings() {
        let mut s = open();
        write_config(s.config.path(), &Config { counts: false, ..s.app.config.clone() });
        later(&mut s);
        std::fs::remove_file(config_path(&s)).expect("remove");

        s.app.refresh(Instant::now() + Duration::from_secs(4));

        assert!(!s.app.config.counts);
    }
}
