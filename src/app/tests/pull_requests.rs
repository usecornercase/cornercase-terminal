use super::*;
use crate::app::pull_requests::{ASK_AGENT, COPY_LINK, DONE};
use crate::pull_request::State;
use crate::test_util::{clone_of, fake_gh, fixture, git};

const VIEW: &str = fixture!("gh/2.101.0/pr-view.json");

const URL: &str = "https://github.com/acme/web-shop/pull/482";

struct Repo {
    app: App,
    rx: Receiver<AppEvent>,
    tmp: TempDir,
    _remote: TempDir,
    _clone: TempDir,
}

impl Repo {
    fn worktree(&self) -> u64 {
        self.app.projects[0].workspaces[1].id
    }

    fn calls(&self) -> Vec<String> {
        let calls = std::fs::read_to_string(self.tmp.path().join("gh.calls")).unwrap_or_default();
        calls.lines().map(str::to_string).collect()
    }

    fn ask(&mut self, what: &str) {
        self.app.ask_pull_requests(Instant::now());
        pump_until(&mut self.app, &self.rx, what, |a| a.pulls.asking.is_empty());
    }
}

fn gh_answering(dir: &Path, found: &str) -> PathBuf {
    fake_gh(
        dir,
        &format!(
            "pwd >> \"$0.calls\"\nif [ \"$(git rev-parse --abbrev-ref HEAD)\" = feat/x ]; then {found}\nelse echo \
             'no pull requests found for branch' >&2; exit 1; fi"
        ),
    )
}

fn printing() -> String {
    format!("cat <<'EOF'\n{VIEW}\nEOF")
}

fn repo_with(found: &str, configure: impl FnOnce(&mut Config)) -> Repo {
    let remote = git_repo(&[("README", "hi")]);
    let (clone_dir, clone) = clone_of(remote.path());
    let tmp = TempDir::new();
    let checkout = tmp.path().join("feat-x");
    git(&clone, &["worktree", "add", "--quiet", "-b", "feat/x", &checkout.display().to_string()]);
    let mut config = Config { gh: gh_answering(tmp.path(), found).display().to_string(), ..Config::default() };
    configure(&mut config);
    let (mut app, rx) = app_in(&clone, write_config(tmp.path(), &config));
    app.sync_worktrees();
    assert_eq!(app.projects[0].workspaces.len(), 2);
    Repo { app, rx, tmp, _remote: remote, _clone: clone_dir }
}

fn repo() -> Repo {
    repo_with(&printing(), |_| {})
}

fn found() -> Repo {
    let mut repo = repo();
    repo.ask("gh answers");
    assert!(repo.app.pull_request(repo.worktree()).is_some());
    repo
}

fn row_text(app: &mut App, row: WorkspaceRow) -> String {
    let r = row_rect(app, row);
    let terminal = render(app, AREA);
    let buffer = terminal.backend().buffer();
    (r.left()..r.right()).map(|x| buffer[(x, r.y)].symbol().to_string()).collect()
}

fn open_dialog(repo: &mut Repo) {
    right_click_row(&mut repo.app, WorkspaceRow::Workspace(1));
    pick(&mut repo.app, "pull request #482");
}

mod asking {
    use super::*;

    #[test]
    fn a_worktree_with_one_shows_its_number_and_failing_checks_on_its_row() {
        let mut repo = found();

        let pull = repo.app.pull_request(repo.worktree()).expect("a pull request");
        assert_eq!((pull.number, pull.state, pull.url.as_str()), (482, State::Open, URL));
        assert!(row_text(&mut repo.app, WorkspaceRow::Workspace(1)).contains("#482✗"));
    }

    #[test]
    fn the_default_branch_is_not_asked() {
        let repo = found();

        let worktree = repo.app.projects[0].workspaces[1].path.clone();
        assert_eq!(repo.calls(), [worktree.display().to_string()]);
    }

    #[test]
    fn a_repo_without_a_remote_is_not_asked() {
        let repo = git_repo(&[]);
        let tmp = TempDir::new();
        let checkout = tmp.path().join("feat-x");
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "feat/x", &checkout.display().to_string()]);
        let config = Config { gh: gh_answering(tmp.path(), &printing()).display().to_string(), ..Config::default() };
        let (mut app, rx) = app_in(repo.path(), write_config(tmp.path(), &config));
        app.sync_worktrees();

        app.ask_pull_requests(Instant::now());
        drain(&mut app, &rx);

        assert!(app.pulls.asking.is_empty());
        assert!(!tmp.path().join("gh.calls").exists());
    }

    #[test]
    fn it_is_not_asked_again_until_its_time_comes() {
        let mut repo = found();

        repo.ask("nothing more to ask");

        assert_eq!(repo.calls().len(), 1);
    }

    #[test]
    fn a_branch_without_one_shows_nothing() {
        let mut repo = repo_with("echo 'no pull requests found for branch \"feat/x\"' >&2; exit 1", |_| {});

        repo.ask("gh answers");

        assert_eq!(repo.app.pull_request(repo.worktree()), None);
        assert!(!row_text(&mut repo.app, WorkspaceRow::Workspace(1)).contains('#'));
    }

    #[test]
    fn a_failing_gh_says_why_and_waits_before_asking_again() {
        let mut repo = repo_with("echo 'error connecting to api.github.com' >&2; exit 1", |_| {});

        repo.ask("gh answers");
        repo.ask("nothing more to ask");

        assert_eq!(repo.calls().len(), 1);
        assert_eq!(repo.app.pull_requests_note(), "error connecting to api.github.com");
    }

    #[test]
    fn a_missing_gh_says_to_install_it() {
        let mut repo = repo_with(&printing(), |c| c.gh = "/nonexistent/gh".into());

        repo.ask("gh answers");

        assert_eq!(repo.app.pull_requests_note(), "install gh, GitHub's CLI, to see them");
    }

    #[test]
    fn turned_off_nothing_is_asked() {
        let mut repo = repo_with(&printing(), |c| c.pull_requests = false);

        repo.ask("nothing to ask");

        assert_eq!(repo.calls(), Vec::<String>::new());
    }

    #[test]
    fn turning_them_off_hides_them() {
        let mut repo = found();

        repo.app.config.pull_requests = false;
        repo.app.ask_pull_requests(Instant::now());

        assert_eq!(repo.app.pull_request(repo.worktree()), None);
    }

    #[test]
    fn another_branch_drops_the_one_shown_at_once() {
        let mut repo = found();
        let path = repo.app.projects[0].workspaces[1].path.clone();

        git(&path, &["checkout", "--quiet", "-b", "feat/y"]);
        repo.app.ask_pull_requests(Instant::now());

        assert_eq!(repo.app.pull_request(repo.worktree()), None);
    }

    #[test]
    fn scripts_see_it_in_the_status() {
        let repo = found();

        let report = repo.app.report(None);

        let pull = report.projects[0].workspaces[1].pull_request.as_ref().expect("in the report");
        assert_eq!((pull.number, pull.state.as_str(), pull.checks.as_deref()), (482, "open", Some("failing")));
        assert_eq!(pull.failed, ["tests (ubuntu-24.04)"]);
        assert_eq!(report.projects[0].workspaces[0].pull_request, None);
    }
}

mod dialog {
    use super::*;

    fn notes(app: &App) -> Vec<String> {
        let Some(Overlay::PullRequest { workspace, scroll }) = app.overlay else { panic!("the dialog is not open") };
        let Some(ui::Overlay::Update(update)) = app.pull_request_view(workspace, scroll) else { panic!("no view") };
        update.notes.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn opens_from_the_workspace_menu_with_its_checks() {
        let mut repo = found();

        open_dialog(&mut repo);

        assert_eq!(
            notes(&repo.app),
            [
                "open · 1 failed, 1 pending, 2 passed, 1 skipped",
                "",
                "✗ tests (ubuntu-24.04)  CI",
                "• tests (macos-15)  CI",
                "✓ lint  CI",
                "✓ review-bot",
                "- deploy  Pages",
            ]
        );
    }

    #[test]
    fn copy_link_copies_the_pull_request_link() {
        let mut repo = found();
        open_dialog(&mut repo);

        click(&mut repo.app, ui::update_buttons(AREA, COPY_LINK, DONE)[0].as_position());

        assert!(repo.app.overlay.is_none());
        assert_eq!(repo.app.outbox.host_writes, [clipboard::osc52(URL)]);
        assert_eq!(toast(&repo.app), Some(COPIED));
    }

    #[test]
    fn ask_agent_hands_over_the_failed_checks() {
        let mut repo = found();
        open_dialog(&mut repo);

        click(&mut repo.app, ui::update_extra(AREA, COPY_LINK, DONE, ASK_AGENT).as_position());

        let text = "pull request #482 failed its checks: tests (ubuntu-24.04) \
                    https://github.com/acme/web-shop/actions/runs/1001/job/2001";
        assert_eq!(repo.app.outbox.host_writes, [clipboard::osc52(text)]);
        assert_eq!(toast(&repo.app), Some(NO_AGENT));
    }

    #[test]
    fn done_closes_it() {
        let mut repo = found();
        open_dialog(&mut repo);

        click(&mut repo.app, ui::update_buttons(AREA, COPY_LINK, DONE)[1].as_position());

        assert!(repo.app.overlay.is_none());
    }

    #[test]
    fn it_closes_when_its_workspace_goes() {
        let mut repo = found();
        open_dialog(&mut repo);

        repo.app.projects[0].workspaces.remove(1);
        repo.app.ask_pull_requests(Instant::now());

        assert!(repo.app.overlay.is_none());
    }

    #[test]
    fn opening_it_asks_again_at_once() {
        let mut repo = found();

        open_dialog(&mut repo);
        pump_until(&mut repo.app, &repo.rx, "gh answers", |a| a.pulls.asking.is_empty());

        assert_eq!(repo.calls().len(), 2);
    }

    #[test]
    fn the_menu_of_a_workspace_without_one_has_no_entry() {
        let mut repo = found();

        right_click_row(&mut repo.app, WorkspaceRow::Workspace(0));

        assert_eq!(menu_labels(&repo.app), ["rename workspace"]);
    }
}

mod by_key {
    use super::*;

    fn keys(app: &mut App, typed: &str) {
        app.config.prefix_key = "ctrl+]".into();
        send_key(app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        for c in typed.chars() {
            send_key(app, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    #[test]
    fn g_p_opens_the_dialog_of_the_workspace_shown() {
        let mut repo = found();
        repo.app.projects[0].active = 1;

        keys(&mut repo.app, "gp");

        assert!(
            matches!(repo.app.overlay, Some(Overlay::PullRequest { workspace, .. }) if workspace == repo.worktree())
        );
    }

    #[test]
    fn g_p_without_one_says_so() {
        let mut repo = found();

        keys(&mut repo.app, "gp");

        assert!(repo.app.overlay.is_none());
        assert_eq!(toast(&repo.app), Some("no pull request for main"));
    }
}
