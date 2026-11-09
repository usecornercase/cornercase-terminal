use rstest::rstest;

use super::*;

const VIEWER: u64 = 31;

#[rstest]
#[case::with_a_prompt(json!({"name": "Pair", "prompt": "Fix the login form", "focus": true}))]
#[case::on_an_issue(json!({"name": "Pair", "issue": "#7", "focus": true}))]
fn a_focused_run_in_a_view_starts_in_its_project_and_shows_its_orchestrator_there(#[case] args: Value) {
    let gh_dir = TempDir::new();
    let gh = crate::test_util::fake_gh(gh_dir.path(), &format!("cat <<'EOF'\n{ISSUE}\nEOF"));
    let mut team = Team::configured(FAKE, |config, _| config.gh = gh.display().to_string());
    team.app.issues.clear_env_tokens();
    let elsewhere = TempDir::new();
    team.app.open_project(elsewhere.path().to_path_buf(), AREA).expect("the window's project");
    let project = team.app.projects[0].id;
    let view = open_view(&mut team.app, VIEWER, &json!({"cols": 100, "rows": 30, "project": project}));
    let before = (team.app.focus(), team.app.active);

    let text = json!({"contract": 1, "view": view, "command": "flow-run", "args": args}).to_string();
    team.app.request(CLIENT, &text, Some(AREA), Instant::now());
    let answer = reply_later(&mut team.app, &team.rx).expect("the run");

    assert_eq!(answer["project"], project);
    assert_eq!(answer["projection"], 2);
    assert_eq!(team.app.view_ids(view).tab, answer["members"][0]["tab"].as_u64());
    assert_eq!((team.app.focus(), team.app.active), before);
    assert_eq!(team.app.screen.side, None);
}
