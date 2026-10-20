use std::time::{Duration, Instant, SystemTime};

use super::*;
use crate::agents::stats::Stats;
use crate::test_util::{fake_gh, git, git_repo};

const GH: &str = r#"case "$1 $2" in
'auth status') echo '{"hosts":{"github.com":[{"state":"success","active":true,"login":"ana"}]}}';;
'api graphql') echo "{\"data\":{\"search\":{\"issueCount\":2,\"nodes\":[{\"number\":1,\"mergedAt\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"repository\":{\"nameWithOwner\":\"acme/web-shop\"}},{\"number\":2,\"mergedAt\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"repository\":{\"nameWithOwner\":\"other/repo\"}}]}}}";;
esac"#;

struct Repo {
    app: App,
    rx: Receiver<AppEvent>,
    _dirs: (TempDir, TempDir),
}

fn repo(url: Option<&str>, configure: impl FnOnce(&mut Config)) -> Repo {
    let repo = git_repo(&[("README", "hi")]);
    if let Some(url) = url {
        git(repo.path(), &["remote", "add", "origin", url]);
    }
    let tmp = TempDir::new();
    let mut config = Config { gh: fake_gh(tmp.path(), GH).display().to_string(), ..Config::default() };
    configure(&mut config);
    let (mut app, rx) = app_in(repo.path(), write_config(tmp.path(), &config));
    app.agents.stats = Stats::new(SystemTime::now() - Duration::from_secs(3_600));
    Repo { app, rx, _dirs: (repo, tmp) }
}

fn ask(repo: &mut Repo) {
    repo.app.ask_merged(Instant::now());
    pump_until(&mut repo.app, &repo.rx, "the merged pull requests arrive", |a| !a.git.merged.asking());
}

const SHOP: &str = "https://github.com/acme/web-shop.git";

#[test]
fn counts_the_pull_requests_of_the_open_projects_only() {
    let mut repo = repo(Some(SHOP), |_| {});

    ask(&mut repo);

    let view = repo.app.stats_view(SystemTime::now());
    let row = view.merged.expect("a row");
    assert_eq!((row.day, row.week, row.total, row.complete), (1, 1, 1, true));
    assert_eq!(row.accounts.len(), 1);
}

#[test]
fn the_json_the_share_link_and_the_note_carry_it() {
    let mut repo = repo(Some(SHOP), |_| {});
    ask(&mut repo);
    let now = SystemTime::now();

    let view = repo.app.stats_view(now);

    assert!(view.share_link().contains("#v3"));
    assert_eq!(repo.app.merged_note(), "from 1 GitHub account");
    let json = serde_json::to_value(&view).expect("json");
    assert_eq!(json["merged_pull_requests"]["total"], 1);
}

#[test]
fn a_project_without_a_remote_never_runs_gh() {
    let mut repo = repo(None, |_| {});

    repo.app.ask_merged(Instant::now());

    assert!(!repo.app.git.merged.asking());
    assert!(repo.app.stats_view(SystemTime::now()).merged.is_none());
}

#[test]
fn turning_the_setting_off_clears_everything() {
    let mut repo = repo(Some(SHOP), |_| {});
    ask(&mut repo);

    repo.app.config.merged_pull_requests = false;
    repo.app.ask_merged(Instant::now());

    assert!(repo.app.stats_view(SystemTime::now()).merged.is_none());
}

#[test]
fn without_gh_nothing_shows_and_the_note_says_why() {
    let mut repo = repo(Some(SHOP), |c| c.gh = "/nonexistent/gh".into());

    ask(&mut repo);

    assert!(repo.app.stats_view(SystemTime::now()).merged.is_none());
    assert!(repo.app.merged_note().contains("gh"), "{}", repo.app.merged_note());
}
