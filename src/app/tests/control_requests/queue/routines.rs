use super::*;
use crate::config::Config;

struct Shop {
    app: App,
    _rx: Receiver<AppEvent>,
    repo: TempDir,
    _config: TempDir,
}

fn shop() -> Shop {
    let repo = git_repo(&[("README", "hello")]);
    let config = TempDir::new();
    let settings = Config {
        agent_commands: [
            ("mine".to_string(), "/bin/true".to_string()),
            ("claude".to_string(), "/nonexistent/claude".to_string()),
        ]
        .into(),
        ..Config::default()
    };
    let (mut app, rx) = new_app(write_config(config.path(), &settings));
    app.open_project(repo.path().to_path_buf(), AREA).expect("open the shop");
    Shop { app, _rx: rx, repo, _config: config }
}

fn entry(extra: &Value) -> Value {
    let mut entry =
        json!({"zone": "UTC", "prompt": "review the open pull requests", "agent": "claude", "repeat": "2h"});
    if let (Some(base), Some(more)) = (entry.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    entry
}

fn add(app: &mut App, extra: &Value) -> Value {
    ok(app, "queue", &json!({"action": "add", "entry": entry(extra)}))
}

fn listed(app: &mut App, id: u64) -> Value {
    let list = ok(app, "queue", &json!({"action": "list"}));
    list["entries"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["id"] == id))
        .cloned()
        .expect("entry")
}

#[test]
fn a_routine_is_added_pending_and_lists_its_repeat_and_next_time() {
    let mut shop = shop();
    let id = add(&mut shop.app, &json!({}))["id"].as_u64().expect("id");
    let routine = listed(&mut shop.app, id);
    assert_eq!((routine["repeat"].clone(), routine["state"].clone()), (json!("2h"), json!("pending")));
    assert_eq!(routine["next"], routine["at"]);
    assert_eq!(routine["place"]["project"], json!(shop.repo.path()));
    assert!(routine["at"].as_i64().expect("at") > crate::time::now() + 7000);
    assert!(routine.get("paused").is_none() && routine.get("cleanup").is_none());
}

#[test]
fn what_cannot_be_a_routine_is_refused_with_its_reason() {
    let mut shop = shop();
    let cases = [
        (json!({"repeat": "10m"}), "30 minutes"),
        (json!({"issue": "#7"}), "issue"),
        (json!({"at": "in 2h"}), "--at"),
        (json!({"worktree": "x"}), "worktree"),
        (json!({"cleanup": true, "agent": "mine"}), "cannot tell when mine finishes"),
        (json!({"flow": "Quick", "agent": "claude"}), "flow chooses its own"),
        (json!({"flow": "No such flow", "agent": null}), "no flow named"),
        (json!({"prompt": ""}), "needs --prompt"),
        (json!({"submit": false}), "keep a draft"),
    ];
    for (extra, why) in cases {
        let error = refused(&mut shop.app, "queue", &json!({"action": "add", "entry": entry(&extra)}));
        assert!(error.message.contains(why), "{extra}: {}", error.message);
    }
    assert_eq!(shop.app.panels.queued.queue().entries, []);
}

#[test]
fn a_plain_folder_cannot_have_routines() {
    let (mut app, _rx, dirs) = app_with(1);
    let error = refused(&mut app, "queue", &json!({"action": "add", "entry": entry(&json!({}))}));
    assert!(error.message.contains("plain folder"), "{}", error.message);
    drop(dirs);
}

#[test]
fn a_flow_and_a_cleanup_without_a_repeat_are_refused() {
    let mut shop = shop();
    let one_off = json!({"zone": "UTC", "prompt": "go", "agent": "claude", "at": "in 2h"});
    for extra in [json!({"flow": "Quick"}), json!({"cleanup": true})] {
        let mut entry = one_off.clone();
        entry.as_object_mut().expect("object").extend(extra.as_object().expect("object").clone());
        let error = refused(&mut shop.app, "queue", &json!({"action": "add", "entry": entry}));
        assert!(error.message.contains("--every"), "{}", error.message);
    }
}

#[test]
fn pausing_hides_the_next_time_and_resuming_lands_on_the_grid_after_now() {
    let mut shop = shop();
    let id = add(&mut shop.app, &json!({}))["id"].as_u64().expect("id");
    let first = listed(&mut shop.app, id)["at"].as_i64().expect("at");
    assert_eq!(queue(&mut shop.app, "pause", id), json!({"id": id}));
    let paused = listed(&mut shop.app, id);
    assert!(paused.get("next").is_none());
    assert_eq!(paused["paused"], json!(true));
    queue(&mut shop.app, "resume", id);
    let resumed = listed(&mut shop.app, id);
    assert_eq!((resumed["next"].as_i64(), resumed.get("paused")), (Some(first), None));
    let error = refused(&mut shop.app, "queue", &json!({"action": "pause", "entry": 99}));
    assert_eq!(error.code, "failed");
}

#[test]
fn an_unchanged_repeat_keeps_the_time_and_a_changed_one_moves_it() {
    let mut shop = shop();
    let id = add(&mut shop.app, &json!({"repeat": "day 09:00"}))["id"].as_u64().expect("id");
    let first = listed(&mut shop.app, id)["at"].as_i64().expect("at");
    let edit = |app: &mut App, repeat: &str| {
        let entry = json!({"zone": "UTC", "repeat": repeat, "prompt": "review again"});
        ok(app, "queue", &json!({"action": "edit", "entry": {"id": id, "add": entry}}));
        listed(app, id)
    };
    let same = edit(&mut shop.app, "day 09:00");
    assert_eq!((same["at"].as_i64(), same["prompt"].clone()), (Some(first), json!("review again")));
    let moved = edit(&mut shop.app, "weekday 18:30");
    assert_ne!(moved["at"].as_i64(), Some(first));
    assert_eq!(moved["repeat"], json!("weekday 18:30"));
}

#[test]
fn a_run_cannot_be_edited_and_a_one_off_cannot_become_a_routine() {
    let mut shop = shop();
    let id = add(&mut shop.app, &json!({}))["id"].as_u64().expect("id");
    let run = shop.app.panels.queued.run_now(id, crate::time::now()).expect("run");
    let edit = json!({"id": run, "add": {"zone": "UTC", "prompt": "x"}});
    let error = refused(&mut shop.app, "queue", &json!({"action": "edit", "entry": edit}));
    assert!(error.message.contains(&format!("run of routine {id}")), "{}", error.message);
    let project = shop.app.projects[0].id;
    let entry = json!({"zone": "UTC", "prompt": "x", "agent": "claude", "at": "in 3h", "project": project});
    let one_off = ok(&mut shop.app, "queue", &json!({"action": "add", "entry": entry}));
    let edit = json!({"id": one_off["id"], "add": {"zone": "UTC", "repeat": "2h"}});
    let error = refused(&mut shop.app, "queue", &json!({"action": "edit", "entry": edit}));
    assert!(error.message.contains("cannot become a routine"), "{}", error.message);
}

#[test]
fn removing_a_routine_answers_it_with_its_runs_and_restore_puts_them_back() {
    let mut shop = shop();
    let id = add(&mut shop.app, &json!({}))["id"].as_u64().expect("id");
    shop.app.panels.queued.run_now(id, crate::time::now()).expect("run");
    let before = shop.app.panels.queued.queue().entries.clone();
    let removed = queue(&mut shop.app, "remove", id);
    assert_eq!(removed["entries"].as_array().map(Vec::len), Some(2));
    assert_eq!(shop.app.panels.queued.queue().entries, []);
    restore(&mut shop.app, &removed);
    assert_eq!(shop.app.panels.queued.queue().entries, before);
}

#[test]
fn the_app_feed_lists_the_next_time_as_queue_list_does() {
    let mut shop = shop();
    add(&mut shop.app, &json!({}));
    let list = ok(&mut shop.app, "queue", &json!({"action": "list"}));
    let state = shop.app.app_state();
    assert_eq!(serde_json::to_value(&state.queue).expect("json"), list["entries"]);
}

#[test]
fn a_chat_app_that_is_away_hears_when_a_run_fails() {
    let mut shop = shop();
    let mut phone = Phone::paired(&mut shop.app);
    ok(&mut shop.app, "away", &json!({"off": false}));
    phone.read(&mut shop.app);
    shop.app.set_queue_area(AREA);
    let id = add(&mut shop.app, &json!({}))["id"].as_u64().expect("id");
    let at = listed(&mut shop.app, id)["at"].as_i64().expect("at");
    shop.app.refresh_at(Instant::now(), at);
    let said: Vec<String> = phone.read(&mut shop.app).iter().map(|(_, order)| html(order).to_string()).collect();
    assert!(said.iter().any(|text| text.contains("routine review the open pull requests failed")), "{said:?}");
}

#[test]
fn running_a_routine_answers_the_new_run_and_a_failing_run_never_fails_the_routine() {
    let mut shop = shop();
    let id = add(&mut shop.app, &json!({}))["id"].as_u64().expect("id");
    let at = listed(&mut shop.app, id)["at"].clone();
    let answer = queue(&mut shop.app, "run", id);
    let run = answer["run"].as_u64().expect("the new run");
    assert_eq!(answer["id"], json!(id));
    let (routine, run) = (listed(&mut shop.app, id), listed(&mut shop.app, run));
    assert_eq!((&routine["state"], &routine["at"]), (&json!("pending"), &at));
    assert_eq!((&run["state"], &run["routine"]), (&json!("failed"), &json!(id)));
    let error = refused(&mut shop.app, "queue", &json!({"action": "run", "entry": 99}));
    assert_eq!(error.code, "failed");
}

#[test]
fn a_mode_cannot_be_sent_to_a_routine_that_keeps_its_flow() {
    let mut shop = shop();
    let id = add(&mut shop.app, &json!({"flow": "Quick", "agent": null}))["id"].as_u64().expect("id");
    let edit = json!({"id": id, "add": {"zone": "UTC", "mode": "plan"}});
    let error = refused(&mut shop.app, "queue", &json!({"action": "edit", "entry": edit}));
    assert!(error.message.contains("flow chooses its own"), "{}", error.message);
}
