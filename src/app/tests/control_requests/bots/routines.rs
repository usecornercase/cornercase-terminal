use super::*;

fn queue(b: &mut Bots, args: &Value) -> Value {
    asked(&mut b.app, &b.rx, "queue", args).unwrap_or_else(|e| panic!("queue {args}: {} {}", e.code, e.message))
}

fn queue_refused(b: &mut Bots, args: &Value) -> Refused {
    match asked(&mut b.app, &b.rx, "queue", args) {
        Ok(value) => panic!("{args} was taken: {value}"),
        Err(refused) => refused,
    }
}

fn add(fields: &Value) -> Value {
    json!({"action": "add", "entry": fields})
}

#[test]
fn a_bots_routine_is_added_with_no_open_project_and_listed_with_its_bot() {
    let mut b = bots();
    b.new_prs();
    let added = queue(
        &mut b,
        &add(&json!({"bot": "PRs", "repeat": "weekday 09:00", "prompt": "Review PRs", "label": "Review PRs"})),
    );
    let id = added["id"].as_u64().expect("id");
    let listed = queue(&mut b, &json!({"action": "list"}));
    let entry = &listed["entries"][0];
    assert_eq!(
        (entry["id"].as_u64(), entry["place"]["bot"].as_str(), entry["repeat"].as_str()),
        (Some(id), Some("prs"), Some("weekday 09:00"))
    );
    assert_eq!(entry["agent"], "claude");
    let bots = b.ask("bot", &json!({"action": "list"}));
    assert_eq!(bots["bots"][0]["routines"], 1);
    assert_eq!(bots["bots"][0]["next"], entry["next"]);
    assert!(bots["bots"][0]["next"].is_i64());
}

#[test]
fn a_reminder_is_a_one_off_entry_of_the_bot() {
    let mut b = bots();
    b.new_prs();
    let added = queue(&mut b, &add(&json!({"bot": "prs", "at": "in 2h", "prompt": "check whether #482 merged"})));
    let listed = queue(&mut b, &json!({"action": "list"}));
    assert_eq!(listed["entries"][0]["id"], added["id"]);
    assert_eq!(listed["entries"][0]["state"], "pending");
    let bots = b.ask("bot", &json!({"action": "list"}));
    assert_eq!((bots["bots"][0]["routines"].as_u64(), bots["bots"][0]["next"].is_i64()), (Some(0), true));
}

#[test]
fn what_a_bot_does_not_take_is_refused_and_an_unknown_bot_is_not_found() {
    let mut b = bots();
    b.new_prs();
    for (field, value) in [
        ("project", json!(1)),
        ("workspace", json!(1)),
        ("worktree", json!("x")),
        ("issue", json!("#1")),
        ("flow", json!("Quick")),
        ("cleanup", json!(true)),
        ("agent", json!("codex")),
        ("mode", json!("careful")),
        ("fresh", json!(true)),
        ("accounts", json!({"claude": "home"})),
    ] {
        let mut fields = json!({"bot": "prs", "repeat": "2h", "prompt": "x"});
        fields[field] = value;
        let refused = queue_refused(&mut b, &add(&fields));
        assert!(refused.message.starts_with("a bot's queue entry takes no "), "{field}: {}", refused.message);
    }
    let missing = queue_refused(&mut b, &add(&json!({"bot": "nobody", "repeat": "2h", "prompt": "x"})));
    assert_eq!(missing.code, "not-found");
    let empty = queue_refused(&mut b, &add(&json!({"bot": "prs", "repeat": "2h"})));
    assert!(empty.message.contains("needs --prompt"), "{}", empty.message);
    let often = queue_refused(&mut b, &add(&json!({"bot": "prs", "repeat": "10m", "prompt": "x"})));
    assert!(often.message.contains("at most every 30 minutes"), "{}", often.message);
}

#[test]
fn an_entry_cannot_move_between_bots_or_become_a_project_entry() {
    let mut b = bots();
    b.new_prs();
    b.ask("bot", &json!({"action": "new", "name": "docs", "agent": "claude"}));
    let id = queue(&mut b, &add(&json!({"bot": "prs", "repeat": "2h", "prompt": "x"})))["id"].as_u64().expect("id");
    let edit = |fields: Value| json!({"action": "edit", "entry": {"id": id, "add": fields}});
    let moved = queue_refused(&mut b, &edit(json!({"bot": "docs"})));
    assert!(moved.message.contains("cannot move between a bot and a project"), "{}", moved.message);
    let to_project = queue_refused(&mut b, &edit(json!({"project": 1})));
    assert!(to_project.message.contains("takes no project"), "{}", to_project.message);
    queue(&mut b, &edit(json!({"prompt": "y", "repeat": "3h"})));
    let listed = queue(&mut b, &json!({"action": "list"}));
    assert_eq!(listed["entries"][0]["prompt"], "y");
}

#[test]
fn removing_a_bot_answers_its_entries_with_one_undo_that_brings_them_back() {
    let mut b = bots();
    b.new_prs();
    queue(&mut b, &add(&json!({"bot": "prs", "repeat": "2h", "prompt": "x"})));
    queue(&mut b, &add(&json!({"bot": "prs", "at": "in 5h", "prompt": "y"})));
    let removed = b.ask("bot", &json!({"action": "remove", "bot": "prs"}));
    assert_eq!(removed["bot"], "prs");
    assert_eq!(removed["entries"].as_array().map(Vec::len), Some(2));
    let undo = removed["undo"].as_u64().expect("undo");
    assert_eq!(queue(&mut b, &json!({"action": "list"}))["entries"], json!([]));
    let restored = queue(&mut b, &json!({"action": "restore", "entry": undo}));
    assert_eq!(restored["entries"].as_array().map(Vec::len), Some(2));
}

#[test]
fn a_bot_without_entries_removes_with_no_undo() {
    let mut b = bots();
    b.new_prs();
    let removed = b.ask("bot", &json!({"action": "remove", "bot": "prs"}));
    assert_eq!(removed["entries"], json!([]));
    assert!(removed.get("undo").is_none());
}

#[test]
fn queue_run_over_the_socket_refuses_an_answered_entry_and_leaves_it_alone() {
    let mut b = bots();
    b.new_prs();
    let id = queue(&mut b, &add(&json!({"bot": "prs", "at": "in 2h", "prompt": "x"})))["id"].as_u64().expect("id");
    b.app.panels.queued.answered(id, Some("s-1".into()), crate::queue::outcome::Outcome::default());
    let refused = queue_refused(&mut b, &json!({"action": "run", "entry": id}));
    assert_eq!(refused.message, "this queue entry has already started");
    let listed = queue(&mut b, &json!({"action": "list"}));
    assert_eq!(
        (listed["entries"][0]["state"].as_str(), listed["entries"][0]["session"].as_str()),
        (Some("answered"), Some("s-1"))
    );
    assert_eq!(b.argvs(), Vec::<String>::new());
}
