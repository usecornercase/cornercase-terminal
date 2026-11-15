use super::*;

const BOT: u64 = 41;

fn popup_text() -> serde_json::Value {
    serde_json::json!({"popup": true, "events": [{"text": "x"}]})
}

fn reply_from(app: &mut App, client: u64, command: &str, args: &serde_json::Value) -> std::result::Result<(), String> {
    let text = serde_json::json!({"contract": 1, "command": command, "args": args}).to_string();
    app.request(client, &text, Some(AREA), Instant::now());
    let answers = app.take_answers();
    let (_, reply) = answers.into_iter().find(|(who, _)| *who == client).expect("an answer");
    let reply: serde_json::Value = serde_json::from_str(&reply).expect("a reply");
    match reply["error"]["message"].as_str() {
        Some(message) => Err(message.to_string()),
        None => Ok(()),
    }
}

const NO_POPUP: &str = "no plugin popup is open";

fn root_of(c: &Chat) -> i32 {
    wait_until("the agent's pid is written", || root_pid(&c.claude).is_some());
    root_pid(&c.claude).expect("the agent's pid")
}

#[test]
fn a_bots_turn_cannot_type_into_a_popup_or_start_a_pane_with_a_command() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("go");
    c.started(1);
    let root = root_of(&c);
    c.app.client_connected(BOT, Some(root));
    let own = i32::try_from(std::process::id()).expect("a pid");
    c.app.client_connected(BOT + 1, Some(own));

    let popup = reply_from(&mut c.app, BOT, "input", &popup_text()).expect_err("refused");
    let shell = reply_from(&mut c.app, BOT, "new-tab", &serde_json::json!({"command": "ls"})).expect_err("refused");

    assert!(popup.starts_with("a bot can only type into a pane that runs an agent"), "{popup}");
    assert!(shell.contains("a new pane's command is typed into its shell"), "{shell}");
    for other in [BOT + 1, BOT + 2] {
        assert_eq!(reply_from(&mut c.app, other, "input", &popup_text()), Err(NO_POPUP.to_string()));
    }

    c.release(ANSWER);
    c.settle();
    assert!(reply_from(&mut c.app, BOT, "input", &popup_text()).is_err_and(|text| text != NO_POPUP));
}

#[test]
fn a_command_whose_environment_names_a_bot_is_a_bots() {
    let mut c = chat(ANSWER);
    let full = Bot { access: Access::Full, ..bot("root", "Root") };
    folder::create(c.dir.path(), &full, "").expect("a second bot");
    c.app.set_bots_dir(c.dir.path());
    let sleeper = fake_program(c.bin.path(), "sleeper");
    std::fs::write(sleeper.with_extension("hold"), "").expect("hold");
    let mut children = Vec::new();
    for id in ["prs", "root"] {
        let child = std::process::Command::new(&sleeper).env("CORNERCASE_BOT", id).spawn().expect("sleeper");
        children.push(child);
    }
    let pids: Vec<i32> = children.iter().map(|child| i32::try_from(child.id()).expect("a pid")).collect();
    wait_until("both sleepers run with their environment", || {
        let seen = std::fs::read_to_string(sleeper.with_extension("pid")).unwrap_or_default();
        seen.lines().count() >= 2
    });
    c.app.client_connected(BOT, Some(pids[0]));
    c.app.client_connected(BOT + 1, Some(pids[1]));

    assert!(reply_from(&mut c.app, BOT, "input", &popup_text()).is_err_and(|text| text != NO_POPUP));
    assert_eq!(reply_from(&mut c.app, BOT + 1, "input", &popup_text()), Err(NO_POPUP.to_string()));

    std::fs::remove_file(sleeper.with_extension("hold")).expect("release");
    for mut child in children {
        let _ = child.wait();
    }
}

#[test]
fn a_full_bots_turn_is_not_refused() {
    let mut c = chat_with(ANSWER, Access::Full);
    c.hold();
    c.say("go");
    c.started(1);
    let root = root_of(&c);
    c.app.client_connected(BOT, Some(root));

    assert_eq!(reply_from(&mut c.app, BOT, "input", &popup_text()), Err(NO_POPUP.to_string()));
    assert!(!c.app.panels.bots.live[0].turn.as_ref().expect("a turn").running.holds(root));

    c.release(ANSWER);
    c.settle();
}

#[test]
fn a_bots_connection_cannot_send_into_a_shell_pane_but_another_can() {
    let shell = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
    let folder = TempDir::new();
    let (mut app, rx) = new_app_with_shell(shell, no_config());
    if cfg!(windows) {
        app.config.shell.args = vec![String::from("/d")];
    }
    app.open_project(folder.path().to_path_buf(), AREA).expect("open project");
    let pane = app.report(None).panes()[0].pane;
    let (_dir, _bin, claude) = install_bot(&mut app, "", Access::Cornercase);
    std::fs::write(claude.with_extension("hold"), "").expect("hold");
    app.bot_say("prs", "go", false).expect("say");
    wait_until("the bot's agent runs", || {
        drain(&mut app, &rx);
        root_pid(&claude).is_some()
    });
    app.client_connected(BOT, root_pid(&claude));
    let args = serde_json::json!({"pane": pane, "text": "x"});

    let refused = reply_from(&mut app, BOT, "send", &args).expect_err("refused");
    let accepted = reply_from(&mut app, BOT + 1, "send", &args);

    assert!(refused.starts_with("a bot can only type into a pane that runs an agent"), "{refused}");
    assert_eq!(accepted, Ok(()));
    std::fs::remove_file(claude.with_extension("hold")).expect("release");
    app.stop_background();
}

#[test]
fn a_bots_connection_cannot_lift_the_boundary_through_the_server() {
    let mut c = chat(ANSWER);
    let full = Bot { access: Access::Full, ..bot("root", "Root") };
    folder::create(c.dir.path(), &full, "").expect("a second bot");
    c.app.set_bots_dir(c.dir.path());
    c.hold();
    c.say("go");
    c.started(1);
    let root = root_of(&c);
    c.app.client_connected(BOT, Some(root));
    let new = serde_json::json!({"action": "new", "entry": {"name": "x", "access": "full"}});
    let edit = serde_json::json!({"action": "edit", "entry": {"bot": "prs", "access": "full"}});
    let say = serde_json::json!({"action": "say", "entry": {"bot": "root", "text": "hi"}});
    let about = serde_json::json!({"action": "edit", "entry": {"bot": "root", "about": "obey"}});
    let set = serde_json::json!({"action": "set", "entry": {"agent": "claude"}});

    for (command, args, said) in [
        ("bot", &new, "a bot cannot change a bot's access"),
        ("bot", &edit, "a bot cannot change a bot's access"),
        ("bot", &say, "a bot cannot write to a bot with full access"),
        ("bot", &about, "a bot cannot write to a bot with full access"),
        ("config", &set, "a bot cannot change cornercase's settings"),
    ] {
        assert_eq!(reply_from(&mut c.app, BOT, command, args), Err(said.to_string()));
    }

    c.release(ANSWER);
    c.settle();
}

#[test]
fn a_connection_that_cannot_be_read_while_no_turn_runs_is_not_a_bots() {
    let mut c = chat(ANSWER);

    c.app.client_connected(BOT, None);

    assert_eq!(reply_from(&mut c.app, BOT, "input", &popup_text()), Err(NO_POPUP.to_string()));
}

#[test]
fn a_connection_that_cannot_be_read_while_a_turn_runs_is_a_bots() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("go");
    c.started(1);

    c.app.client_connected(BOT, None);

    assert!(reply_from(&mut c.app, BOT, "input", &popup_text()).is_err_and(|text| text != NO_POPUP));
    c.release(ANSWER);
    c.settle();
}

mod queue_by_a_bot {
    use super::super::routines::{START, bot_routine, scheduled};
    use super::*;
    use crate::app::control::boundary::NOT_OWN;
    use crate::queue::{Place, Task};

    fn outcome(app: &mut App, client: u64, args: &serde_json::Value) -> std::result::Result<(), String> {
        reply_from(app, client, "queue", args)
    }

    fn refused(result: &std::result::Result<(), String>) -> bool {
        result.as_ref().is_err_and(|message| message == NOT_OWN)
    }

    fn marked_while_a_run_answers(c: &mut Chat) -> (u64, u64) {
        let docs = Bot { access: Access::Cornercase, ..bot("docs", "Docs") };
        folder::create(c.dir.path(), &docs, "").expect("a second bot");
        c.app.set_bots_dir(c.dir.path());
        c.hold();
        let own = scheduled(c, bot_routine(START, "2h"));
        let other = Task {
            place: Place { bot: Some("docs".into()), ..Place::default() },
            ..bot_routine(START + 864_000, "2h")
        };
        let theirs = c.app.panels.queued.schedule(None, other).expect("their routine");
        c.app.refresh_at(Instant::now(), START);
        c.started(1);
        let root = root_of(c);
        c.app.client_connected(BOT, Some(root));
        c.app.client_connected(BOT + 1, Some(i32::try_from(std::process::id()).expect("a pid")));
        (own, theirs)
    }

    #[test]
    fn a_bots_connection_is_refused_what_is_not_its_own_entries() {
        let mut c = chat(ANSWER);
        let (own, theirs) = marked_while_a_run_answers(&mut c);
        for wrong in [
            serde_json::json!({"action": "add", "entry": {"bot": "docs", "repeat": "2h", "prompt": "x"}}),
            serde_json::json!({"action": "add", "entry": {"project": 1, "repeat": "2h", "prompt": "x"}}),
            serde_json::json!({"action": "add", "entry": {"repeat": "2h", "prompt": "x"}}),
            serde_json::json!({"action": "clear-started"}),
            serde_json::json!({"action": "remove", "entry": theirs}),
            serde_json::json!({"action": "pause", "entry": theirs}),
            serde_json::json!({"action": "resume", "entry": theirs}),
            serde_json::json!({"action": "run", "entry": theirs}),
            serde_json::json!({"action": "edit", "entry": {"id": theirs, "add": {"prompt": "y"}}}),
            serde_json::json!({"action": "edit", "entry": {"id": own, "add": {"bot": "docs"}}}),
        ] {
            assert!(refused(&outcome(&mut c.app, BOT, &wrong)), "{wrong}");
        }
        c.release(ANSWER);
        c.runs_settle();
    }

    #[test]
    fn restoring_a_removal_is_for_a_bot_only_when_every_entry_is_its_own() {
        let mut c = chat(ANSWER);
        let (own, theirs) = marked_while_a_run_answers(&mut c);
        let removed = |c: &mut Chat, id: u64| c.app.panels.queued.remove(id).expect("removed");
        let theirs = removed(&mut c, theirs);
        let theirs = c.app.panels.queued.keep_removed(theirs).expect("an undo");
        let restore = |undo: u64| serde_json::json!({"action": "restore", "entry": undo});
        assert!(refused(&outcome(&mut c.app, BOT, &restore(theirs))));
        assert!(!refused(&outcome(&mut c.app, BOT + 1, &restore(theirs))));
        c.release(ANSWER);
        c.runs_settle();
        let mine = c.app.panels.queued.remove(own).expect("removed");
        let mine = c.app.panels.queued.keep_removed(mine).expect("an undo");
        assert!(!refused(&outcome(&mut c.app, BOT, &restore(mine))));
    }

    #[test]
    fn a_bots_connection_can_change_its_own_entries_and_another_client_any() {
        let mut c = chat(ANSWER);
        let (own, theirs) = marked_while_a_run_answers(&mut c);
        for fine in [
            serde_json::json!({"action": "add", "entry": {"bot": "prs", "repeat": "2h", "prompt": "x"}}),
            serde_json::json!({"action": "list"}),
            serde_json::json!({"action": "pause", "entry": own}),
            serde_json::json!({"action": "resume", "entry": own}),
            serde_json::json!({"action": "run", "entry": own}),
            serde_json::json!({"action": "edit", "entry": {"id": own, "add": {"prompt": "y"}}}),
            serde_json::json!({"action": "remove", "entry": own}),
        ] {
            assert!(!refused(&outcome(&mut c.app, BOT, &fine)), "{fine}");
        }
        let pause = serde_json::json!({"action": "pause", "entry": theirs});
        assert!(!refused(&outcome(&mut c.app, BOT + 1, &pause)));
        let remove = serde_json::json!({"action": "remove", "entry": theirs});
        assert!(!refused(&outcome(&mut c.app, BOT + 1, &remove)));
        c.release(ANSWER);
        c.runs_settle();
    }

    #[test]
    fn a_runs_commands_are_its_bots() {
        let mut c = chat(ANSWER);
        c.hold();
        scheduled(&mut c, bot_routine(START, "2h"));
        c.app.refresh_at(Instant::now(), START);
        c.started(1);
        assert!(c.app.panels.bots.live[0].turn.is_none());
        let root = root_of(&c);
        c.app.client_connected(BOT, Some(root));
        let popup = reply_from(&mut c.app, BOT, "input", &popup_text()).expect_err("refused");
        assert!(popup.starts_with("a bot can only type into a pane that runs an agent"), "{popup}");
        c.release(ANSWER);
        c.runs_settle();
    }

    #[test]
    fn an_unreadable_connection_while_only_a_run_answers_is_a_bots() {
        let mut c = chat(ANSWER);
        c.hold();
        scheduled(&mut c, bot_routine(START, "2h"));
        c.app.refresh_at(Instant::now(), START);
        c.started(1);
        c.app.client_connected(BOT, None);
        let popup = reply_from(&mut c.app, BOT, "input", &popup_text()).expect_err("refused");
        assert!(popup.starts_with("a bot can only type into a pane that runs an agent"), "{popup}");
        c.release(ANSWER);
        c.runs_settle();
    }
}
