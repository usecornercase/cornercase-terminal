use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;
use crate::issues::secrets;
use crate::test_util::{FakeHttp, fake_gh};

pub(super) fn assert_only_linear(s: &Setup) {
    let connected: Vec<Source> =
        Source::REMOTE.into_iter().filter(|source| s.app.issues.account(*source).is_some()).collect();
    assert_eq!(connected, [Source::Linear]);
}

pub(super) fn accounts_with_linear(s: &mut Setup, source: Source, workspace: &str) {
    s.app.learn_account(source, &Account { handle: "Ana".into(), workspace: workspace.into() });
    s.app.learn_account(Source::Linear, &Account { handle: "ana".into(), workspace: "b".into() });
}

pub(super) fn shortcut_with_token(s: &mut Setup) -> FakeHttp {
    let server = shortcut(s, Vec::new());
    secrets::write(&secrets_file(s), "shortcut_token", "t0k").expect("save token");
    open_loaded(s, Source::Shortcut);
    server
}

pub(super) const LIST: &str = r#"[
            {"number":7,"title":"Fix the login","state":"OPEN","labels":[{"name":"bug"}],"author":{"login":"ana"},
             "updatedAt":"2026-09-19T12:00:00Z","url":"https://github.com/acme/shop/issues/7"},
            {"number":3,"title":"Dark mode","state":"OPEN","labels":[],"author":{"login":"luis"},
             "updatedAt":"2026-09-01T12:00:00Z","url":"https://github.com/acme/shop/issues/3"}
        ]"#;
pub(super) const VIEW: &str = r#"{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],
            "author":{"login":"ana"},"updatedAt":"","url":"u","body":"It **breaks**.",
            "comments":[{"author":{"login":"bo"},"body":"Same here","createdAt":""}]}"#;
pub(super) const STORIES: &str = r#"{"data":[{"id":482,"name":"Returns page crashes","app_url":"https://app.shortcut.com/acme/story/482",
            "updated_at":"2026-09-30T00:00:00Z"}],"next":null}"#;

pub(super) const FAKE_AGENT: &str = "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\n\
            printf '%s' \"$answer\" > answered\nprintf '\\033[2J\\033[H'\n\
            printf 'agent ready> '\nread line\nprintf '%s' \"$line\" > got\n";

pub(super) struct Setup {
    pub(super) app: App,
    pub(super) rx: Receiver<AppEvent>,
    pub(super) worktrees: TempDir,
    pub(super) config: TempDir,
    _dir: TempDir,
}

pub(super) fn setup(git: bool, gh_script: &str) -> Setup {
    let dir = if git { git_repo(&[("README", "hi")]) } else { TempDir::new() };
    let (worktrees, config) = (TempDir::new(), TempDir::new());
    let gh = fake_gh(config.path(), &format!("touch \"$0.called\"\necho \"$@\" >> \"$0.args\"\n{gh_script}"));
    let agent = config.path().join("agent");
    crate::test_util::write_executable(&agent, FAKE_AGENT);
    let settings = Config {
        worktrees_dir: worktrees.path().display().to_string(),
        agent: "fake".into(),
        agent_commands: [("fake".to_string(), agent.display().to_string())].into(),
        gh: gh.display().to_string(),
        accept_trust_prompts: true,
        ..Config::default()
    };
    let config_path = write_config(config.path(), &settings);
    let (mut app, rx) = app_in(dir.path(), config_path);
    app.issues.clear_env_tokens();
    Setup { app, rx, worktrees, config, _dir: dir }
}

pub(super) fn listing() -> Setup {
    setup(true, &format!("if [ \"$2\" = view ]; then cat <<'EOF'\n{VIEW}\nEOF\nelse cat <<'EOF'\n{LIST}\nEOF\nfi"))
}

pub(super) fn shortcut(s: &mut Setup, routes: Vec<(&'static str, u16, &'static str)>) -> FakeHttp {
    let mut all = vec![
        ("GET /api/v3/member", 200, crate::test_util::SHORTCUT_MEMBER),
        ("GET /api/v3/members", 200, "[]"),
        ("GET /api/v3/workflows", 200, "[]"),
        ("GET /api/v3/search/stories", 200, STORIES),
    ];
    all.splice(0..0, routes);
    fake_shortcut(&mut s.app, all)
}

pub(super) fn secrets_file(s: &Setup) -> PathBuf {
    paths::secrets(&s.config.path().join("config.json"))
}

pub(super) fn browser(app: &App) -> &Browser {
    let Some(Overlay::Issues(b)) = &app.overlay else { panic!("the list is not open") };
    b
}

pub(super) fn shown(app: &App) -> Vec<String> {
    browser(app).shown().iter().map(|i| i.key.clone()).collect()
}

pub(super) fn loaded(app: &App, source: Source) -> bool {
    browser(app).lists.get(&source).is_some_and(|l| !l.loading)
}

pub(super) fn open_list(s: &mut Setup) {
    let issues = app_areas(&s.app).issues;
    click(&mut s.app, issues.as_position());
}

pub(super) fn open_loaded(s: &mut Setup, source: Source) {
    open_list(s);
    pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, source));
}

pub(super) fn enter(s: &mut Setup) {
    send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
}

pub(super) fn start(s: &mut Setup) {
    open_loaded(s, Source::Github);
    enter(s);
    enter(s);
    enter(s);
    pump_until(&mut s.app, &s.rx, "the issue workspace opens", |a| a.projects[0].workspaces.len() == 2);
}

pub(super) fn checkout(s: &Setup) -> PathBuf {
    let repo = s.app.projects[0].path.file_name().expect("repo name").to_owned();
    s.worktrees.path().join(repo).join("issue-7-fix-the-login")
}

fn pump(s: &mut Setup) {
    drain(&mut s.app, &s.rx);
    s.app.refresh(Instant::now());
}

const LAUNCH_WAIT: Duration = Duration::from_secs(20);

fn wait_typed(s: &mut Setup, text: &str) {
    wait_until_within("the command is typed", LAUNCH_WAIT, || {
        pump(s);
        s.app.agents.launches.is_empty() && screen(&mut s.app).replace('\n', "").contains(text)
    });
}

fn to_tab_after_open(s: &mut Setup, tab: IssueTab) {
    open_list(s);
    to_tab(s, tab);
}

pub(super) fn to_tab(s: &mut Setup, tab: IssueTab) {
    let i = browser(&s.app).tabs.iter().position(|t| *t == tab).expect("the tab is shown");
    for _ in 0..i {
        send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
    }
}

mod github {
    use super::*;

    #[test]
    fn the_button_lists_the_open_issues_from_gh() {
        let mut s = listing();
        open_loaded(&mut s, Source::Github);
        assert_eq!(shown(&s.app), ["#7", "#3"]);
    }

    #[test]
    fn outside_git_gh_is_not_asked() {
        let mut s = setup(false, "echo '[]'");
        open_list(&mut s);
        assert!(browser(&s.app).lists.is_empty());
        assert!(!s.config.path().join("gh.called").exists());
    }

    #[test]
    fn what_gh_says_shows_in_the_list() {
        let mut s = setup(true, "echo 'no git remotes found' >&2; exit 1");
        open_loaded(&mut s, Source::Github);
        let error = browser(&s.app).lists[&Source::Github].error.clone();
        assert_eq!(error.as_deref(), Some("no git remotes found"));
    }

    #[test]
    fn typing_filters_the_issues() {
        let mut s = listing();
        open_loaded(&mut s, Source::Github);
        type_text(&mut s.app, "dark");
        assert_eq!(shown(&s.app), ["#3"]);
    }

    #[test]
    fn a_second_opening_shows_the_last_list_at_once() {
        let mut s = listing();
        open_loaded(&mut s, Source::Github);
        send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

        open_list(&mut s);

        assert_eq!(shown(&s.app), ["#7", "#3"]);
    }

    #[test]
    fn esc_closes_the_list() {
        let mut s = listing();
        open_loaded(&mut s, Source::Github);
        send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(s.app.overlay.is_none());
    }

    #[test]
    fn the_last_tab_opens_again() {
        let mut s = listing();
        open_list(&mut s);
        send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
        send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

        open_list(&mut s);

        assert_eq!(browser(&s.app).current(), IssueTab::One(Source::Github));
    }
}

mod filters_and_places {
    use super::*;

    fn gh_args(s: &Setup) -> String {
        std::fs::read_to_string(s.config.path().join("gh.args")).unwrap_or_default()
    }

    fn with_people() -> Setup {
        setup(
            true,
            &format!(
                "case \"$1\" in api) printf 'zoe\\nana\\n';; *) if [ \"$2\" = view ]; then cat <<'EOF'\n{VIEW}\nEOF\nelse cat <<'EOF'\n{LIST}\nEOF\nfi;; esac"
            ),
        )
    }

    fn open_people(s: &mut Setup) {
        click(&mut s.app, ui::issue_toggles(ui::issues_area(AREA), &["closed", "people"])[1].as_position());
    }

    #[test]
    fn a_person_picked_goes_into_the_gh_search() {
        let mut s = with_people();
        to_tab_after_open(&mut s, IssueTab::One(Source::Github));
        open_people(&mut s);
        pump_until(&mut s.app, &s.rx, "the people load", |a| browser(a).members.contains_key(&Source::Github));
        enter(&mut s);
        type_text(&mut s.app, "zoe");
        enter(&mut s);
        send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

        pump_until(&mut s.app, &s.rx, "the filtered list loads", |a| loaded(a, Source::Github));

        assert!(gh_args(&s).contains("--assignee zoe"), "{}", gh_args(&s));
        assert_eq!(s.app.issues.people().github[0], issues::Who::Person("zoe".into()));
    }

    #[test]
    fn a_story_starts_in_the_project_picked_for_it() {
        let mut s = setup(true, "echo '[]'");
        let notes = TempDir::new();
        s.app.open_project(notes.path().to_path_buf(), AREA).expect("open notes");
        s.app.active = 0;
        let _server = shortcut_with_token(&mut s);
        enter(&mut s);
        enter(&mut s);
        let folder = notes.path().file_name().and_then(|n| n.to_str()).expect("name").to_string();
        type_text(&mut s.app, &folder);

        enter(&mut s);

        let project = &s.app.projects[1];
        assert_eq!((s.app.overlay.is_none(), s.app.active, project.workspaces[0].tabs.len()), (true, 1, 2));
        assert_eq!(project.workspaces[0].tabs[1].label(&s.app.config), "sc-482 Returns page crashes");
    }
}

mod remembering {
    use super::*;

    fn people() -> People {
        People { github: [issues::Who::Me, issues::Who::Anyone], ..People::default() }
    }

    fn toggle(s: &mut Setup, i: usize) {
        let toggles = ui::issue_toggles(ui::issues_area(AREA), &["closed", "people"]);
        click(&mut s.app, toggles[i].as_position());
    }

    #[test]
    fn the_tab_and_the_toggles_are_saved_in_the_session() {
        let mut s = listing();
        open_list(&mut s);
        to_tab(&mut s, IssueTab::One(Source::Github));
        toggle(&mut s, 0);

        let saved = s.app.state().issues;

        let expected = IssuesState { tab: Some("github".into()), closed: true, people: People::default() };
        assert_eq!(saved, Some(expected));
    }

    #[test]
    fn a_restored_session_opens_the_same_tab_and_toggles() {
        let mut s = listing();
        let saved = State {
            issues: Some(IssuesState { tab: Some("github".into()), closed: false, people: people() }),
            ..s.app.state()
        };
        let (mut app, _rx) = empty_app();
        app.restore(&saved, AREA);
        s.app.issues.restore(app.issues.saved().as_ref());

        open_list(&mut s);

        assert_eq!(
            (browser(&s.app).current(), browser(&s.app).people.clone()),
            (IssueTab::One(Source::Github), people())
        );
    }

    #[test]
    fn the_last_list_comes_back_from_disk_after_a_restart() {
        let mut s = listing();
        let cache = s.config.path().join("issues.json");
        s.app.set_issue_cache(cache.clone());
        open_loaded(&mut s, Source::Github);
        let dir = s.app.projects[0].path.clone();

        let (mut app, _rx) = app_in(&dir, s.config.path().join("config.json"));
        app.set_issue_cache(cache);
        click(&mut app, areas().issues.as_position());

        let shown: Vec<String> = browser(&app).shown().iter().map(|i| i.key.clone()).collect();
        assert_eq!(shown, ["#7", "#3"]);
    }
}

mod reading {
    use super::*;

    #[test]
    fn copy_url_sends_it_to_the_outer_terminal() {
        let mut s = listing();
        open_loaded(&mut s, Source::Github);
        enter(&mut s);
        let labels = ["start", "raw", "copy url", "back"];

        click(&mut s.app, ui::issue_buttons(ui::issues_area(AREA), &labels)[2].as_position());

        let url = "https://github.com/acme/shop/issues/7";
        assert_eq!(s.app.take_host_writes(), [clipboard::osc52(url)]);
        assert_eq!(browser(&s.app).notice.as_deref(), Some(format!("copied {url}").as_str()));
    }

    #[test]
    fn enter_reads_the_issue_with_its_comments() {
        let mut s = listing();
        open_loaded(&mut s, Source::Github);

        enter(&mut s);

        pump_until(&mut s.app, &s.rx, "the issue loads", |a| {
            matches!(&browser(a).screen, Screen::Detail { detail: Some(_), .. })
        });
        let Screen::Detail { detail: Some(Ok(detail)), .. } = &browser(&s.app).screen else {
            panic!("the issue did not load")
        };
        assert_eq!((detail.body.as_str(), detail.comments.len()), ("It **breaks**.", 1));
    }
}

mod starting {
    use super::*;

    #[test]
    fn opens_a_workspace_in_its_own_worktree() {
        let mut s = listing();
        start(&mut s);
        let expected = checkout(&s).canonicalize().expect("checkout");
        let project = &s.app.projects[0];
        let workspace = &project.workspaces[1];
        assert_eq!(
            (workspace.path.clone(), workspace.worktree, workspace.label(), git::branch(&workspace.path)),
            (expected, true, "#7 Fix the login".into(), Some("issue-7-fix-the-login".into()))
        );
        assert_eq!((s.app.overlay.is_none(), project.active, workspace.tabs.len()), (true, 1, 1));
    }

    #[test]
    fn the_start_button_starts_the_selected_issue() {
        let mut s = listing();
        open_loaded(&mut s, Source::Github);
        send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
        let start =
            ui::issue_buttons(ui::issues_area(AREA), &browser(&s.app).labels(issues_dialog::room(AREA).buttons))[0];
        click(&mut s.app, start.as_position());
        enter(&mut s);
        pump_until(&mut s.app, &s.rx, "the issue workspace opens", |a| a.projects[0].workspaces.len() == 2);
        assert_eq!(s.app.projects[0].workspaces[1].label(), "#3 Dark mode");
    }

    const URL: &str = "https://github.com/acme/shop/issues/7";

    fn got(s: &Setup) -> Option<String> {
        std::fs::read_to_string(checkout(s).join("got")).ok()
    }

    #[test]
    fn the_agent_trusts_the_folder_and_gets_the_prompt_typed() {
        let mut s = listing();
        start(&mut s);
        wait_typed(&mut s, &format!("agent ready> {URL}"));
        assert_eq!(got(&s), None);
    }

    #[test]
    fn a_launch_waiting_for_your_answer_lets_the_server_sleep() {
        let mut s = listing();
        s.app.config.accept_trust_prompts = Config::default().accept_trust_prompts;
        start(&mut s);

        wait_until_within("the launch waits for you", LAUNCH_WAIT, || {
            pump(&mut s);
            s.app.agents.launches.iter().all(Launch::waits_for_you) && !s.app.agents.launches.is_empty()
        });

        assert_eq!(s.app.tick(Instant::now()), None);
    }

    #[test]
    fn by_default_you_answer_the_trust_question_and_then_the_prompt_is_typed() {
        let mut s = listing();
        s.app.config.accept_trust_prompts = Config::default().accept_trust_prompts;
        start(&mut s);
        wait_until_within("the agent asks", LAUNCH_WAIT, || {
            pump(&mut s);
            screen(&mut s.app).contains("Do you trust the files")
        });

        "yes".chars().for_each(|c| send_key(&mut s.app, KeyCode::Char(c), KeyModifiers::NONE));
        enter(&mut s);

        wait_typed(&mut s, &format!("agent ready> {URL}"));
        assert_eq!(std::fs::read_to_string(checkout(&s).join("answered")).ok().as_deref(), Some("yes"));
    }

    #[test]
    fn enter_sends_the_typed_prompt_to_the_agent() {
        let mut s = listing();
        start(&mut s);
        wait_typed(&mut s, &format!("agent ready> {URL}"));

        enter(&mut s);

        wait_until("the agent gets the prompt", || got(&s).as_deref() == Some(URL));
    }

    #[test]
    fn submit_sends_the_prompt_by_itself() {
        let mut s = listing();
        s.app.config.submit = true;
        start(&mut s);

        wait_until_within("the agent gets the prompt", LAUNCH_WAIT, || {
            pump(&mut s);
            got(&s).as_deref() == Some(URL)
        });
    }

    #[test]
    fn the_prompt_template_is_filled_in() {
        let mut s = listing();
        s.app.config.prompt = "Fix {key}: {title}".into();
        start(&mut s);
        wait_typed(&mut s, "agent ready> Fix #7: Fix the login");
    }

    #[test]
    fn starting_it_again_goes_to_its_workspace() {
        let mut s = listing();
        start(&mut s);
        s.app.projects[0].active = 0;

        open_loaded(&mut s, Source::Github);
        enter(&mut s);
        enter(&mut s);
        enter(&mut s);

        let project = &s.app.projects[0];
        assert_eq!((s.app.overlay.is_none(), project.workspaces.len(), project.active), (true, 2, 1));
    }

    #[test]
    fn git_errors_stay_in_the_list() {
        let mut s = listing();
        std::fs::create_dir_all(checkout(&s)).expect("create the checkout folder");
        open_loaded(&mut s, Source::Github);
        enter(&mut s);
        enter(&mut s);
        enter(&mut s);

        pump_until(
            &mut s.app,
            &s.rx,
            "the start fails",
            |a| matches!(&a.overlay, Some(Overlay::Issues(b)) if !b.starting && b.error.is_some()),
        );
        assert_eq!(s.app.projects[0].workspaces.len(), 1);
    }

    #[test]
    fn without_git_a_story_starts_in_a_new_tab() {
        let mut s = setup(false, "echo '[]'");
        let _server = shortcut_with_token(&mut s);

        enter(&mut s);
        enter(&mut s);

        let workspace = &s.app.projects[0].workspaces[0];
        assert_eq!(
            (s.app.overlay.is_none(), workspace.tabs.len(), workspace.active, workspace.tabs[1].label(&s.app.config)),
            (true, 2, 1, "sc-482 Returns page crashes".into())
        );
        wait_typed(&mut s, "agent ready> https://app.shortcut.com/acme/story/482");
    }
}

mod tokens {
    use super::*;

    fn type_token(s: &mut Setup, token: &str) {
        open_list(s);
        to_tab(s, IssueTab::One(Source::Shortcut));
        type_text(&mut s.app, token);
        enter(s);
    }

    #[test]
    fn a_good_token_is_saved_and_lists_the_stories() {
        let mut s = setup(false, "echo '[]'");
        let _server = shortcut(&mut s, Vec::new());

        type_token(&mut s, "t0k");

        pump_until(&mut s.app, &s.rx, "the stories load", |a| loaded(a, Source::Shortcut));
        assert_eq!(shown(&s.app), ["sc-482"]);
        assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token").as_deref(), Some("t0k"));
    }

    #[test]
    fn a_rejected_token_is_not_saved() {
        let mut s = setup(false, "echo '[]'");
        let _server = shortcut(&mut s, vec![("GET /api/v3/member", 401, "{}")]);

        type_token(&mut s, "bad");

        pump_until(&mut s.app, &s.rx, "the check fails", |a| {
            browser(a).forms.get(&Source::Shortcut).is_some_and(|f| f.error.is_some())
        });
        assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token"), None);
    }

    #[test]
    fn a_token_from_the_environment_needs_no_form() {
        let mut s = setup(false, "echo '[]'");
        let _server = shortcut(&mut s, Vec::new());
        s.app.issues.set_env_token(Source::Shortcut, "env-token");

        open_loaded(&mut s, Source::Shortcut);

        assert_eq!(shown(&s.app), ["sc-482"]);
    }

    #[test]
    fn disconnect_forgets_the_saved_token() {
        let mut s = setup(false, "echo '[]'");
        let _server = shortcut(&mut s, Vec::new());
        secrets::write(&secrets_file(&s), "shortcut_token", "t0k").expect("save token");
        open_list(&mut s);
        to_tab(&mut s, IssueTab::One(Source::Shortcut));

        click(
            &mut s.app,
            ui::issue_buttons(ui::issues_area(AREA), &["start", "refresh", "disconnect", "cancel"])[2].as_position(),
        );

        assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token"), None);
        assert!(!browser(&s.app).connections.contains_key(&Source::Shortcut));
    }
}

mod jira {
    use super::*;

    fn configured_jira() -> (Setup, FakeHttp) {
        let mut s = setup(false, "echo '[]'");
        let server = jira(&mut s);
        s.app.config.jira_site = "acme.atlassian.net".into();
        s.app.config.jira_email = "ana@acme.dev".into();
        (s, server)
    }

    const MYSELF: &str = r#"{"accountId":"a1","displayName":"Ana"}"#;
    const SEARCH: &str = r#"{"issues":[{"key":"SHOP-482","fields":{"summary":"Returns page crashes",
                "updated":"2026-09-30T00:00:00.000+0000"}}],"isLast":true}"#;

    fn jira(s: &mut Setup) -> FakeHttp {
        let server =
            FakeHttp::start(vec![("GET /rest/api/3/myself", 200, MYSELF), ("GET /rest/api/3/search/jql", 200, SEARCH)]);
        s.app.issues.api_at(Source::Jira, &server.url());
        server
    }

    fn type_site_and_email(s: &mut Setup) {
        open_list(s);
        to_tab(s, IssueTab::One(Source::Jira));
        type_text(&mut s.app, "acme");
        enter(s);
        type_text(&mut s.app, "ana@acme.dev");
        enter(s);
    }

    fn answer(s: &mut Setup, wanted: impl Fn(&AppEvent) -> bool) {
        loop {
            let event = s.rx.recv_timeout(Duration::from_secs(10)).expect("the answer arrives");
            let found = wanted(&event);
            s.app.handle_event(event, AREA).expect("handle event");
            if found {
                return;
            }
        }
    }

    fn saved_config(s: &Setup) -> Config {
        config::load(&s.config.path().join("config.json"))
    }

    #[test]
    fn connecting_saves_the_site_and_the_email_then_the_token_and_lists() {
        let mut s = setup(false, "echo '[]'");
        let server = jira(&mut s);

        type_site_and_email(&mut s);
        let config = saved_config(&s);
        type_text(&mut s.app, "t0k");
        enter(&mut s);

        pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Jira));
        assert_eq!(shown(&s.app), ["SHOP-482"]);
        assert_eq!((config.jira_site.as_str(), config.jira_email.as_str()), ("acme.atlassian.net", "ana@acme.dev"));
        assert_eq!(secrets::read(&secrets_file(&s), "jira_api_token").as_deref(), Some("t0k"));
        let basic = STANDARD.encode(b"ana@acme.dev:t0k").to_lowercase();
        assert!(server.request(0).to_lowercase().contains(&format!("authorization: basic {basic}")));
    }

    #[test]
    fn a_token_from_the_environment_connects_once_the_email_is_typed() {
        let mut s = setup(false, "echo '[]'");
        let _server = jira(&mut s);
        s.app.issues.set_env_token(Source::Jira, "env-token");

        type_site_and_email(&mut s);

        pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Jira));
        assert_eq!(shown(&s.app), ["SHOP-482"]);
    }

    #[test]
    fn a_token_without_a_site_is_not_connected() {
        let mut s = setup(false, "echo '[]'");
        secrets::write(&secrets_file(&s), "jira_api_token", "t0k").expect("save token");

        open_list(&mut s);

        assert!(!browser(&s.app).connections.contains_key(&Source::Jira));
    }

    #[test]
    fn a_list_asked_before_the_site_changed_is_dropped() {
        let (mut s, _server) = configured_jira();
        secrets::write(&secrets_file(&s), "jira_api_token", "t0k").expect("save token");
        open_list(&mut s);
        let key = s.app.cache_key(Source::Jira, browser(&s.app).project, &browser(&s.app).query(Source::Jira));

        s.app.set_config(Config { jira_site: "other.atlassian.net".into(), ..s.app.config.clone() });
        answer(&mut s, |e| matches!(e, AppEvent::Issues(IssueEvent::Listed { source: Source::Jira, .. })));

        let account = s.app.issues.account(Source::Jira).cloned();
        assert_eq!((account, s.app.issues.cache.get(&key)), (None, None));
    }

    #[test]
    fn a_token_checked_against_the_old_email_is_not_saved() {
        let (mut s, _server) = configured_jira();

        s.app.check_token(Source::Jira, Secret("t0k".into()));
        s.app.set_config(Config { jira_email: "bo@acme.dev".into(), ..s.app.config.clone() });
        answer(&mut s, |e| matches!(e, AppEvent::Issues(IssueEvent::TokenChecked { .. })));

        assert_eq!(secrets::read(&secrets_file(&s), "jira_api_token"), None);
    }

    #[test]
    fn a_list_asked_with_the_old_token_is_dropped_once_a_new_one_is_saved() {
        let (mut s, _server) = configured_jira();
        secrets::write(&secrets_file(&s), "jira_api_token", "old").expect("save token");
        open_list(&mut s);
        let key = s.app.cache_key(Source::Jira, browser(&s.app).project, &browser(&s.app).query(Source::Jira));
        let account = Account { handle: "Ana".into(), workspace: "acme.atlassian.net".into() };

        s.app.token_checked(Source::Jira, &Secret("new".into()), Ok(account), AREA).expect("save the new token");
        answer(&mut s, |e| matches!(e, AppEvent::Issues(IssueEvent::Listed { source: Source::Jira, .. })));

        assert_eq!(s.app.issues.cache.get(&key), None);
    }

    #[test]
    fn another_site_forgets_the_account_and_the_lists_of_the_old_one() {
        let mut s = setup(false, "echo '[]'");
        accounts_with_linear(&mut s, Source::Jira, "a");

        s.app.set_config(Config { jira_site: "other.atlassian.net".into(), ..s.app.config.clone() });

        assert_only_linear(&s);
    }
}

mod plane {
    use super::*;

    const ME: &str = r#"{"id":"me-1","display_name":"Ana"}"#;
    const MEMBERS: &str = r#"[{"id":"me-1","display_name":"Ana","email":"ana@acme.dev"}]"#;
    const ITEMS: &str = r#"{"data":[{"identifier":"ENG-42","sequence_id":42,"name":"Login redirect loops",
                "updated_at":"2026-09-30T00:00:00Z","state":{"name":"Todo","group":"unstarted"}}],
                "pagination":{"style":"cursor"},"has_more":false,"next_cursor":null}"#;

    fn plane(s: &mut Setup) -> FakeHttp {
        let server = FakeHttp::start(vec![
            ("GET /api/v1/users/me/", 200, ME),
            ("GET /api/v1/workspaces/acme/members/", 200, MEMBERS),
            ("GET /api/v2/workspaces/acme/work-items/", 200, ITEMS),
        ]);
        s.app.issues.api_at(Source::Plane, &server.url());
        server
    }

    fn type_workspace(s: &mut Setup) {
        open_list(s);
        to_tab(s, IssueTab::One(Source::Plane));
        type_text(&mut s.app, "acme");
        enter(s);
    }

    #[test]
    fn connecting_saves_the_workspace_then_the_key_and_lists() {
        let mut s = setup(false, "echo '[]'");
        let server = plane(&mut s);

        type_workspace(&mut s);
        let config = config::load(&s.config.path().join("config.json"));
        type_text(&mut s.app, "k3y");
        enter(&mut s);

        pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Plane));
        assert_eq!(shown(&s.app), ["ENG-42"]);
        assert_eq!(config.plane_workspace, "acme");
        assert_eq!(secrets::read(&secrets_file(&s), "plane_api_key").as_deref(), Some("k3y"));
        assert!(server.request(0).to_lowercase().contains("x-api-key: k3y"), "{}", server.request(0));
    }

    #[test]
    fn a_key_from_the_environment_connects_once_the_workspace_is_typed() {
        let mut s = setup(false, "echo '[]'");
        let _server = plane(&mut s);
        s.app.issues.set_env_token(Source::Plane, "env-key");

        type_workspace(&mut s);

        pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Plane));
        assert_eq!(shown(&s.app), ["ENG-42"]);
    }

    #[test]
    fn a_key_without_a_workspace_is_not_connected() {
        let mut s = setup(false, "echo '[]'");
        secrets::write(&secrets_file(&s), "plane_api_key", "k3y").expect("save key");

        open_list(&mut s);

        assert!(!browser(&s.app).connections.contains_key(&Source::Plane));
    }

    #[test]
    fn another_workspace_forgets_the_account_of_the_old_one() {
        let mut s = setup(false, "echo '[]'");
        accounts_with_linear(&mut s, Source::Plane, "acme");

        s.app.set_config(Config { plane_workspace: "other".into(), ..s.app.config.clone() });

        assert_only_linear(&s);
    }

    #[test]
    fn a_key_checked_without_a_workspace_says_to_set_it() {
        let mut s = setup(false, "echo '[]'");

        s.app.check_token(Source::Plane, Secret("k3y".into()));

        let event = s.rx.recv_timeout(Duration::from_secs(10)).expect("the answer arrives");
        let result = std::iter::once(event)
            .chain(std::iter::from_fn(|| s.rx.recv_timeout(Duration::from_secs(10)).ok()))
            .find_map(|event| match event {
                AppEvent::Issues(IssueEvent::TokenChecked { result, .. }) => Some(result),
                _ => None,
            })
            .expect("a token check");
        assert_eq!(result.err().map(|e| e.to_string()).as_deref(), Some("set the Plane workspace first"));
    }
}

mod gitlab {
    use super::*;

    const USER: &str = r#"{"id":7,"username":"ana","name":"Ana"}"#;
    const ISSUES: &str = r#"[{"iid":5,"title":"Checkout fails","state":"opened","author":{"username":"ana"},
                "updated_at":"2026-09-30T00:00:00Z","web_url":"https://gitlab.acme.dev/acme/shop/-/issues/5"}]"#;
    const ISSUE: &str = r#"{"iid":5,"title":"Checkout fails","state":"opened","description":"It **fails**."}"#;

    fn gitlab(s: &mut Setup) -> FakeHttp {
        let server = FakeHttp::start(vec![
            ("GET /api/v4/user", 200, USER),
            ("GET /api/v4/projects/acme%2Fshop/issues", 200, ISSUES),
            ("GET /api/v4/projects/acme%2Fshop/issues/5", 200, ISSUE),
            ("GET /api/v4/projects/acme%2Fshop/issues/5/notes", 200, "[]"),
        ]);
        let remote = format!("{}/acme/shop.git", server.url());
        crate::test_util::git(&s.app.projects[0].path, &["remote", "add", "origin", &remote]);
        server
    }

    fn connected(s: &mut Setup) -> FakeHttp {
        let server = gitlab(s);
        s.app.config.gitlab_url = server.url();
        s.app.issues.set_env_token(Source::Gitlab, "env-token");
        to_tab_after_open(s, IssueTab::One(Source::Gitlab));
        pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Gitlab));
        server
    }

    #[test]
    fn connecting_saves_the_server_then_the_token_and_lists_the_remote_project() {
        let mut s = setup(true, "echo '[]'");
        let server = gitlab(&mut s);

        to_tab_after_open(&mut s, IssueTab::One(Source::Gitlab));
        type_text(&mut s.app, &server.url());
        enter(&mut s);
        let config = config::load(&s.config.path().join("config.json"));
        type_text(&mut s.app, "glpat");
        enter(&mut s);

        pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Gitlab));
        assert_eq!(shown(&s.app), ["#5"]);
        assert_eq!(config.gitlab_url, server.url());
        assert_eq!(secrets::read(&secrets_file(&s), "gitlab_token").as_deref(), Some("glpat"));
        assert!(server.request(0).to_lowercase().contains("private-token: glpat"), "{}", server.request(0));
    }

    #[test]
    fn a_token_from_the_environment_lists_at_once() {
        let mut s = setup(true, "echo '[]'");
        let _server = connected(&mut s);
        assert_eq!(shown(&s.app), ["#5"]);
    }

    #[test]
    fn an_issue_starts_in_a_worktree_named_after_its_number() {
        let mut s = setup(true, "echo '[]'");
        let _server = connected(&mut s);

        enter(&mut s);
        enter(&mut s);
        enter(&mut s);

        pump_until(&mut s.app, &s.rx, "the issue workspace opens", |a| a.projects[0].workspaces.len() == 2);
        let workspace = &s.app.projects[0].workspaces[1];
        assert_eq!(
            (workspace.label(), git::branch(&workspace.path)),
            ("#5 Checkout fails".into(), Some("5-checkout-fails".into()))
        );
    }

    #[test]
    fn another_server_forgets_the_account_of_the_old_one() {
        let mut s = setup(false, "echo '[]'");
        accounts_with_linear(&mut s, Source::Gitlab, "gitlab.com");

        s.app.set_config(Config { gitlab_url: "https://gitlab.acme.dev".into(), ..s.app.config.clone() });

        assert_only_linear(&s);
    }
}
