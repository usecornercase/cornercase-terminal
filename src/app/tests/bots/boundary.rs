use super::*;
use crate::app::control::boundary::NO_KEYS;
use crate::protocol::ClientMessage;

const BOT: u64 = 41;

fn popup_text() -> serde_json::Value {
    serde_json::json!({"popup": true, "events": [{"text": "x"}]})
}

fn reply_from(app: &mut App, client: u64, command: &str, args: &serde_json::Value) -> std::result::Result<(), String> {
    reply_in(app, client, None, command, args)
}

fn reply_in(
    app: &mut App,
    client: u64,
    view: Option<u64>,
    command: &str,
    args: &serde_json::Value,
) -> std::result::Result<(), String> {
    let text = serde_json::json!({"contract": 1, "view": view, "command": command, "args": args}).to_string();
    app.request(client, &text, Some(AREA), Instant::now());
    let answers = app.take_answers();
    let (_, reply) = answers.into_iter().find(|(who, _)| *who == client).expect("an answer");
    let reply: serde_json::Value = serde_json::from_str(&reply).expect("a reply");
    match reply["error"]["message"].as_str() {
        Some(message) => Err(message.to_string()),
        None => Ok(()),
    }
}

const NO_CLONE: &str = "a bot cannot clone a repository; clone it yourself and the bot can work in the project";
const NO_POPUP: &str = "no plugin popup is open";

fn root_of(c: &Chat) -> i32 {
    wait_until("the agent's pid is written", || root_pid(&c.claude).is_some());
    root_pid(&c.claude).expect("the agent's pid")
}

fn bots_turn() -> Chat {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("go");
    c.started(1);
    let root = root_of(&c);
    c.app.client_connected(BOT, Some(root));
    c
}

#[test]
fn a_bots_turn_cannot_say_a_view_shows_a_chat() {
    let mut c = bots_turn();
    let own = i32::try_from(std::process::id()).expect("a pid");
    c.app.client_connected(BOT + 1, Some(own));

    let named = reply_from(&mut c.app, BOT, "view", &serde_json::json!({"bot": "prs"}));
    let by_another = reply_from(&mut c.app, BOT + 1, "view", &serde_json::json!({"bot": "prs"}));
    let none = reply_from(&mut c.app, BOT, "view", &serde_json::json!({}));

    assert_eq!(named, Err("a bot cannot show a bot's chat in a view".to_string()));
    assert_eq!((by_another, none), (Ok(()), Ok(())));
    c.release(ANSWER);
    c.settle();
}

#[test]
fn a_bots_turn_cannot_make_visible_a_view_that_names_a_bot_but_can_clear_it() {
    let mut c = bots_turn();
    let hidden = crate::control::OpenView { bot: Some("prs".into()), visible: Some(false), ..Default::default() };
    let opened = c.app.view_request(BOT + 1, &hidden, Some(AREA)).expect("open").expect("a snapshot");
    let view = opened["view"]["id"].as_u64().expect("a view id");
    let shown = reply_in(&mut c.app, BOT, Some(view), "view-set", &serde_json::json!({"visible": true}));
    let cleared = reply_in(&mut c.app, BOT, Some(view), "view-set", &serde_json::json!({"visible": true, "bot": ""}));

    assert_eq!(shown, Err("a bot cannot show a bot's chat in a view".to_string()));
    assert_eq!(cleared, Ok(()));
    c.release(ANSWER);
    c.settle();
}

#[test]
fn a_bots_turn_cannot_type_into_a_popup_or_start_a_pane_with_a_command() {
    let mut c = bots_turn();
    let own = i32::try_from(std::process::id()).expect("a pid");
    c.app.client_connected(BOT + 1, Some(own));

    let popup = reply_from(&mut c.app, BOT, "input", &popup_text()).expect_err("refused");
    let shell = reply_from(&mut c.app, BOT, "new-tab", &serde_json::json!({"command": "ls"})).expect_err("refused");

    assert_eq!(popup, NO_KEYS);
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
    let sleepers = BotSleepers::start(c.bin.path(), &["prs", "root"]);
    let pids = sleepers.pids();
    c.app.client_connected(BOT, Some(pids[0]));
    c.app.client_connected(BOT + 1, Some(pids[1]));

    assert!(reply_from(&mut c.app, BOT, "input", &popup_text()).is_err_and(|text| text != NO_POPUP));
    assert_eq!(reply_from(&mut c.app, BOT + 1, "input", &popup_text()), Err(NO_POPUP.to_string()));

    sleepers.release();
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
    let folder = TempDir::new();
    let (mut app, rx) = system_shell_app();
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
    let setup = serde_json::json!({"action": "setup", "entry": {"bot": "root"}});
    let set = serde_json::json!({"action": "set", "entry": {"agent": "claude"}});
    let clones = TempDir::new();
    let clone = serde_json::json!({"url": "https://example.com/x.git", "parent": clones.path()});
    let stop = "a bot cannot stop its own server";
    c.app.update.updates_mut().enabled = true;
    c.app.config.check_updates = false;

    for (command, args, said) in [
        ("restart-when-idle", &serde_json::json!({}), stop),
        ("update-install", &serde_json::json!({}), stop),
        ("clone", &clone, NO_CLONE),
        ("bot", &new, "a bot cannot change a bot's access"),
        ("bot", &edit, "a bot cannot change a bot's access"),
        ("bot", &say, "a bot cannot write to a bot with full access"),
        ("bot", &about, "a bot cannot write to a bot with full access"),
        ("bot", &setup, "a bot cannot write to a bot with full access"),
        ("config", &set, "a bot cannot change cornercase's settings"),
    ] {
        assert_eq!(reply_from(&mut c.app, BOT, command, args), Err(said.to_string()));
    }
    assert!(!clones.path().join("x").exists());

    c.release(ANSWER);
    c.settle();
}

#[test]
fn another_connection_still_stops_updates_and_clones() {
    let mut c = bots_turn();
    c.app.client_connected(BOT + 1, Some(i32::try_from(std::process::id()).expect("a pid")));
    c.app.update.updates_mut().enabled = true;
    c.app.config.check_updates = false;
    let relative = serde_json::json!({"url": "https://example.com/x.git", "parent": "relative"});

    let install = reply_from(&mut c.app, BOT + 1, "update-install", &serde_json::json!({}));
    let clone = reply_from(&mut c.app, BOT + 1, "clone", &relative);
    let text = serde_json::json!({"contract": 1, "command": "restart-when-idle", "args": {}}).to_string();
    c.app.request(BOT + 1, &text, Some(AREA), Instant::now());

    let install = install.expect_err("no release to install");
    let clone = clone.expect_err("a relative folder");
    assert!(install.contains("release") && !install.contains("bot"), "{install}");
    assert!(clone.contains("must be absolute"), "{clone}");
    assert!(c.app.take_answers().iter().all(|(who, _)| *who != BOT + 1));

    c.release(ANSWER);
    c.settle();
}

#[test]
fn a_bots_stop_messages_are_refused_and_its_connection_stays_a_bots() {
    let mut c = bots_turn();
    let stop = Some("a bot cannot stop its own server");

    assert_eq!(c.app.refused_stop(BOT, &ClientMessage::KillServer), stop);
    assert_eq!(c.app.refused_stop(BOT, &ClientMessage::Restart), stop);
    assert_eq!(c.app.refused_stop(BOT, &ClientMessage::KillServer), stop);
    assert_eq!(c.app.refused_stop(BOT + 1, &ClientMessage::KillServer), None);
    assert_eq!(c.app.refused_stop(BOT, &ClientMessage::Request(String::new())), None);

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
        let mut mixed = c.app.panels.queued.remove(own).expect("removed");
        let mine = c.app.panels.queued.keep_removed(mixed.clone()).expect("an undo");
        assert!(!refused(&outcome(&mut c.app, BOT, &restore(mine))));
        let other = Task { place: Place { bot: Some("docs".into()), ..Place::default() }, ..bot_routine(START, "2h") };
        let id = c.app.panels.queued.schedule(None, other).expect("their routine");
        mixed.extend(c.app.panels.queued.remove(id).expect("removed"));
        let both = c.app.panels.queued.keep_removed(mixed).expect("an undo");
        assert!(refused(&outcome(&mut c.app, BOT, &restore(both))));
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
        assert_eq!(popup, NO_KEYS);
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
        assert_eq!(popup, NO_KEYS);
        c.release(ANSWER);
        c.runs_settle();
    }
}

mod marked {
    use super::*;
    use crate::app::control::boundary::OWN_ONLY;

    const NEVER_ASKS: &str = "a bot cannot start an agent in a mode that never asks before it acts";

    fn shell_app() -> (App, Receiver<AppEvent>) {
        let (mut app, rx) = system_shell_app();
        app.config.agent_commands.insert("claude".into(), "/nonexistent/claude".into());
        app.clients.bots.marked(BOT, "prs");
        (app, rx)
    }

    fn refusal(app: &mut App, client: u64, command: &str, args: &serde_json::Value) -> Option<String> {
        let text = serde_json::json!({"contract": 1, "command": command, "args": args}).to_string();
        app.request(client, &text, Some(AREA), Instant::now());
        let answers = app.take_answers();
        let (_, reply) = answers.into_iter().find(|(who, _)| *who == client)?;
        let reply: serde_json::Value = serde_json::from_str(&reply).expect("a reply");
        reply["error"]["message"].as_str().map(str::to_string)
    }

    fn refused_to_the_bot_only(app: &mut App, command: &str, args: &serde_json::Value, why: &str) {
        let bots = refusal(app, BOT, command, args).unwrap_or_else(|| panic!("{command} was allowed to the bot"));
        assert!(bots.starts_with(why), "{command}: {bots}");
        let others = refusal(app, BOT + 1, command, args);
        assert!(others.is_none_or(|others| !others.starts_with(why)), "{command} was refused to another client");
    }

    fn refused_to_the_bot(app: &mut App, command: &str, args: &serde_json::Value, why: &str) {
        let bots = refusal(app, BOT, command, args).unwrap_or_else(|| panic!("{command} was allowed to the bot"));
        assert!(bots.starts_with(why), "{command}: {bots}");
    }

    #[test]
    fn a_bot_is_refused_what_the_table_closes() {
        let (mut app, _rx) = shell_app();
        let none = serde_json::json!({});
        for (command, args, why) in [
            ("keys", serde_json::json!({"pane": 99, "keys": ["enter"]}), "a bot cannot press keys"),
            ("input", serde_json::json!({"pane": 99, "events": [{"text": "x"}]}), "a bot cannot press keys"),
            ("stop-process", serde_json::json!({"pid": 2_000_000_000}), "a bot cannot stop a process"),
            ("changes-discard", serde_json::json!({"workspace": 99, "path": "x"}), "a bot cannot discard changes"),
            ("agents-forget", serde_json::json!({"workspace": 99}), OWN_ONLY),
            ("tunnel-open", serde_json::json!({"port": 5173}), crate::bots::NO_TUNNELS),
        ] {
            refused_to_the_bot_only(&mut app, command, &args, why);
        }
        for (command, args, why) in [
            ("flow-run", serde_json::json!({"name": "x", "prompt": "y"}), "a bot cannot run a flow"),
            ("flow-delete", serde_json::json!({"name": "x"}), "a bot cannot change cornercase's flows"),
            ("plugin-run", serde_json::json!({"plugin": "x", "action": "y", "revision": 1}), crate::bots::NO_PLUGINS),
            ("worktrees-remove", none.clone(), "a bot cannot remove a worktree"),
            ("account-use", serde_json::json!({"provider": "claude", "account": "x"}), "a bot cannot change your"),
            ("config-pair", none.clone(), "a bot cannot pair a device"),
            ("forum-server", serde_json::json!({"id": "1"}), "a bot cannot change cornercase's settings"),
        ] {
            refused_to_the_bot(&mut app, command, &args, why);
        }
    }

    #[test]
    fn a_bot_is_refused_what_changes_flows_machines_and_what_it_did_not_start() {
        let (mut app, _rx) = shell_app();
        let flows = "a bot cannot change cornercase's flows";
        for (command, args, why) in [
            ("flow-import", serde_json::json!({"text": "x"}), flows),
            ("flow-save", serde_json::json!({"flow": {"name": "x"}}), flows),
            ("close-agent", serde_json::json!({"pane": 99}), OWN_ONLY),
            ("agents-close", serde_json::json!({"workspace": 99}), OWN_ONLY),
            ("close", serde_json::json!({"item": {"project": 99}}), OWN_ONLY),
            ("close", serde_json::json!({"item": {"workspace": 99}}), OWN_ONLY),
            ("machines", serde_json::json!({"action": "remove", "entry": "vps"}), crate::bots::NO_MACHINES),
            ("start", serde_json::json!({"machine": "vps", "prompt": "x"}), crate::bots::ELSEWHERE),
        ] {
            refused_to_the_bot_only(&mut app, command, &args, why);
        }
        for (command, args) in [
            ("flow-import", serde_json::json!({"text": "x", "check": true})),
            ("close-agent", serde_json::json!({"pane": 99, "check": true})),
            ("machines", serde_json::json!({"action": "list"})),
        ] {
            let said = refusal(&mut app, BOT, command, &args);
            assert!(said.as_deref().is_none_or(|said| !said.starts_with("a bot")), "{command}: {said:?}");
        }
    }

    #[test]
    fn a_bot_cannot_open_a_bots_folder_or_a_code_workspace() {
        let (mut app, _rx) = shell_app();
        let bots = TempDir::new();
        std::fs::create_dir_all(bots.path().join("prs")).expect("a bot's folder");
        app.set_bots_dir(bots.path());
        app.clients.bots.marked(BOT, "prs");
        let file = TempDir::new();
        let workspace = file.path().join("x.code-workspace");
        std::fs::write(&workspace, "{\"folders\": []}").expect("a workspace file");
        let other = TempDir::new();

        for (path, why) in [
            (bots.path().to_path_buf(), "a bot cannot open a bot's folder as a project"),
            (bots.path().join("prs"), "a bot cannot open a bot's folder as a project"),
            (workspace, "a bot cannot open a .code-workspace file"),
        ] {
            refused_to_the_bot_only(&mut app, "open", &serde_json::json!({"path": path}), why);
        }
        let measured = serde_json::json!({"path": bots.path().join("prs")});
        let said = refusal(&mut app, BOT, "worktree-size", &measured).expect("refused");
        assert_eq!(said, "a bot cannot measure a folder inside a bot's folder");
        assert_eq!(refusal(&mut app, BOT, "open", &serde_json::json!({"path": other.path()})), None);
        app.stop_background();
    }

    #[test]
    fn a_bot_cannot_start_an_agent_in_a_mode_that_never_asks() {
        let (mut app, _rx) = shell_app();
        let folder = TempDir::new();
        app.open_project(folder.path().to_path_buf(), AREA).expect("open project");
        let project = app.projects[0].id;
        let start = |mode: Option<&str>| serde_json::json!({"project": project, "agent": "claude", "mode": mode, "prompt": "fix it"});

        refused_to_the_bot_only(&mut app, "start", &start(Some("skip permissions (dangerous)")), NEVER_ASKS);
        assert_eq!(refusal(&mut app, BOT, "start", &start(Some("default"))), None);
        app.config.agent_args.insert("claude".into(), vec!["--dangerously-skip-permissions".into()]);
        let refused = refusal(&mut app, BOT, "start", &start(None)).expect("refused");
        assert!(refused.starts_with(NEVER_ASKS), "{refused}");
        assert_eq!(refusal(&mut app, BOT, "start", &start(Some("default"))), None);
        let command = refusal(
            &mut app,
            BOT,
            "start",
            &serde_json::json!({"project": project, "agent": "claude", "mode": "default", "prompt": "!ls"}),
        );
        assert!(command.is_some_and(|said| said.starts_with("a bot writes prompts to an agent")));
        app.stop_background();
    }
}
