use serde_json::Value;

use super::*;
use crate::bots::secrets::{self, Kept, KeyState};
use crate::control::contract::{Code, Refusal};

pub(super) const OWNER: u64 = 7;
pub(super) const BOUND: u64 = 8;
pub(super) const STRANGER: u64 = 9;
pub(super) const VALUE: &str = "hunter2-long-value";

pub(super) struct Kept2 {
    pub(super) app: App,
    _rx: Receiver<AppEvent>,
    pub(super) dir: TempDir,
}

pub(super) fn keeper() -> Kept2 {
    let dir = TempDir::new();
    let bots = dir.path().join("bots");
    for (id, name) in [("campus", "Campus"), ("prs", "PRs")] {
        folder::create(&bots, &bot(id, name), "").expect("create");
    }
    let (mut app, rx) = empty_app();
    app.set_bots_dir(&bots);
    let at = app.panels.bots.by_id("campus").expect("campus");
    app.panels.bots.live[at].turn = Some(turn());
    app.clients.bots.owned(OWNER, "campus");
    app.clients.bots.marked(OWNER, "campus");
    app.clients.bots.marked(BOUND, "campus");
    Kept2 { app, _rx: rx, dir }
}

pub(in crate::app::tests) fn turn() -> crate::app::bots::Turn {
    crate::app::bots::Turn {
        id: 1,
        thread: crate::app::bots::Thread::Chat,
        asked: Vec::new(),
        running: agents::probe::Running::default(),
        stopping: false,
        dropped: Vec::new(),
        started: Instant::now(),
        steps: 0,
        briefed: crate::bots::Briefed::of(&crate::bots::sample("prs", "PRs")),
    }
}

pub(super) fn refusal(result: std::result::Result<Value, Refusal>) -> (Code, String) {
    let refused = result.expect_err("refused");
    (refused.code, refused.message)
}

impl Kept2 {
    pub(super) fn set(&mut self, name: &str, value: &str) -> std::result::Result<Value, Refusal> {
        self.app.bot_secret_set(None, "campus", (name, value))
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.path().join("bots").join(id).join(folder::SECRETS_FILE)
    }

    pub(super) fn reload(&mut self) {
        self.app.set_bots_dir(&self.dir.path().join("bots"));
    }
}

mod who_reads {
    use super::*;

    #[test]
    fn only_the_bot_itself_gets_a_value() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let got = k.app.bot_secret_get(Some(OWNER), "pw").expect("the owner reads");
        assert_eq!((got["bot"].as_str(), got["value"].as_str()), (Some("campus"), Some(VALUE)));
        let said = |result| refusal(result);
        let only = (Code::Failed, secrets::ONLY_ITSELF.to_string());
        assert_eq!(said(k.app.bot_secret_get(Some(BOUND), "pw")), only);
        assert_eq!(said(k.app.bot_secret_get(Some(STRANGER), "pw")), only);
        assert_eq!(said(k.app.bot_secret_get(None, "pw")), only);
    }

    #[test]
    fn a_secret_that_is_not_there_is_not_found() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let (code, message) = refusal(k.app.bot_secret_get(Some(OWNER), "other"));
        assert_eq!(code, Code::NotFound);
        assert!(message.contains("no secret other in campus"), "{message}");
    }

    #[test]
    fn a_name_that_is_not_one_is_never_echoed_back() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let wrong = "Hunter2-Mistaken-Value";
        let got = refusal(k.app.bot_secret_get(Some(OWNER), wrong));
        let removed = refusal(k.app.bot_secret_remove(None, "campus", (Some(wrong), false)));
        for (code, message) in [got, removed] {
            assert!(code == Code::NotFound && !message.contains(wrong), "{message}");
        }
        assert!(refusal(k.app.bot_secret_get(Some(OWNER), "other")).1.contains("no secret other in campus"));
    }

    #[test]
    fn a_grant_ends_with_the_turn_that_made_it() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        assert!(k.app.bot_secret_get(Some(OWNER), "pw").is_ok());
        let at = k.app.panels.bots.by_id("campus").expect("campus");
        k.app.panels.bots.live[at].turn = None;
        assert_eq!(refusal(k.app.bot_secret_get(Some(OWNER), "pw")).1, secrets::ONLY_ITSELF);
        k.app.panels.bots.live[at].turn = Some(turn());
        k.app.bot_settled(at);
        assert!(k.app.clients.bots.owner_of(OWNER).is_some(), "a turn still runs");
        k.app.panels.bots.live[at].turn = None;
        k.app.bot_settled(at);
        assert!(k.app.clients.bots.owner_of(OWNER).is_none());
        k.app.panels.bots.live[at].turn = Some(turn());
        assert_eq!(refusal(k.app.bot_secret_get(Some(OWNER), "pw")).1, secrets::ONLY_ITSELF);
    }

    #[test]
    fn a_full_bots_process_stays_that_bot_and_never_the_user_after_its_turn() {
        const FULL: u64 = 11;
        let mut k = keeper();
        k.app.clients.bots.owned(FULL, "campus");
        let at = k.app.panels.bots.by_id("campus").expect("campus");
        k.app.panels.bots.live[at].turn = None;
        k.app.bot_settled(at);
        let no = (Code::Failed, secrets::NO_CHANGE.to_string());
        assert_eq!(refusal(k.app.bot_secret_set(Some(FULL), "prs", ("x", VALUE))), no);
        assert_eq!(refusal(k.app.bot_secret_remove(Some(FULL), "prs", (None, true))), no);
        assert_eq!(refusal(k.app.bot_secret_list(Some(FULL), "prs")).1, secrets::OWN_ONLY);
        k.app.panels.bots.live[at].turn = Some(turn());
        assert_eq!(refusal(k.app.bot_secret_get(Some(FULL), "pw")).1, secrets::ONLY_ITSELF);
    }

    #[test]
    fn a_bot_lists_only_its_own() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        k.app.clients.bots.marked(STRANGER, "prs");
        assert_eq!(k.app.bot_secret_list(Some(OWNER), "campus").expect("own")["secrets"][0]["name"], "pw");
        assert_eq!(refusal(k.app.bot_secret_list(Some(OWNER), "prs")), (Code::Failed, secrets::OWN_ONLY.to_string()));
        assert_eq!(refusal(k.app.bot_secret_list(Some(STRANGER), "campus")).1, secrets::OWN_ONLY);
        assert_eq!(refusal(k.app.bot_secret_list(Some(OWNER), "nobody")).0, Code::NotFound);
        assert!(k.app.bot_secret_list(None, "campus").is_ok());
    }

    #[test]
    fn no_bot_saves_or_removes() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let no = (Code::Failed, secrets::NO_CHANGE.to_string());
        for client in [OWNER, BOUND] {
            assert_eq!(refusal(k.app.bot_secret_set(Some(client), "campus", ("x", VALUE))), no);
            assert_eq!(refusal(k.app.bot_secret_remove(Some(client), "campus", (Some("pw"), false))), no);
            assert_eq!(refusal(k.app.bot_secret_remove(Some(client), "campus", (None, true))), no);
        }
        assert_eq!(k.app.panels.bots.live[0].secrets.count(), 1);
    }
}

mod the_limits {
    use super::*;

    #[test]
    fn an_empty_value_and_a_big_one_are_refused_with_the_reason() {
        let mut k = keeper();
        assert_eq!(refusal(k.set("pw", "")), (Code::Malformed, String::from("the value is empty")));
        let (code, message) = refusal(k.set("pw", &"a".repeat(8193)));
        assert!(code == Code::Malformed && message.contains("8193"), "{message}");
        assert!(k.set("pw", &"a".repeat(8192)).is_ok());
        assert_eq!(refusal(k.set("Bad Name", "x")).0, Code::Malformed);
    }

    #[test]
    fn the_twenty_first_is_refused_but_a_replacement_is_not() {
        let mut k = keeper();
        for n in 0..20 {
            k.set(&format!("s{n}"), VALUE).expect("set");
        }
        let (code, message) = refusal(k.set("s20", VALUE));
        assert!(code == Code::Conflict && message.contains("20 secrets"), "{message}");
        assert!(k.set("s3", "another-value").is_ok());
        assert_eq!(k.app.panels.bots.live[0].secrets.count(), 20);
    }

    #[test]
    fn a_secret_counts_in_the_bot_list() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        assert_eq!(k.app.bot_infos().iter().map(|info| info.secrets).collect::<Vec<_>>(), [1, 0]);
    }

    #[test]
    fn an_unreadable_file_is_not_the_same_as_none() {
        let mut k = keeper();
        std::fs::write(k.file("campus"), b"garbage").expect("write");
        k.reload();
        let infos = k.app.bot_infos();
        assert_eq!((infos[0].secrets, infos[0].secrets_unreadable), (0, true));
        assert_eq!(serde_json::to_value(&infos[0]).expect("json")["secrets_unreadable"], true);
        let fine = serde_json::to_value(&infos[1]).expect("json");
        assert!(fine.get("secrets_unreadable").is_none() && fine["secrets"] == 0, "{fine}");
    }
}

mod the_file {
    use super::*;

    #[test]
    fn a_file_that_stops_opening_after_the_start_is_not_saved_over_or_erased() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        std::fs::write(k.file("campus"), b"damaged").expect("write");
        assert!(refusal(k.set("other", VALUE)).1.contains("changed on disk"));
        assert!(refusal(k.app.bot_secret_remove(None, "campus", (Some("pw"), false))).1.contains("changed on disk"));
        assert_eq!(std::fs::read(k.file("campus")).expect("kept"), b"damaged");
        assert!(k.app.bot_secret_remove(None, "campus", (None, true)).is_ok());
    }

    #[test]
    fn a_key_that_appears_while_running_is_never_replaced() {
        let mut k = keeper();
        let path = k.app.panels.bots.key_path.clone().expect("a key path");
        std::fs::write(&path, [9u8; 32]).expect("write");
        assert!(refusal(k.set("pw", VALUE)).1.contains("restart cornercase"));
        assert_eq!(std::fs::read(&path).expect("kept"), [9u8; 32]);
        assert!(!k.file("campus").exists());
    }

    #[test]
    fn a_value_a_turn_already_has_stays_redacted_after_it_is_replaced_or_removed() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        k.set("pw", "another-long-value").expect("replace");
        k.app.bot_secret_remove(None, "campus", (Some("pw"), false)).expect("remove");
        let at = k.app.panels.bots.by_id("campus").expect("campus");
        k.app.bot_entry_failed(at, 99, &format!("{VALUE} and another-long-value"), true);
        let written = chat::read(&k.file("campus").with_file_name(folder::CHAT_FILE));
        let texts: Vec<&str> = written.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts, ["[secret pw] and [secret pw]"]);
        k.app.panels.bots.live[at].turn = None;
        k.app.bot_settled(at);
        assert_eq!(k.app.panels.bots.live[at].retired, Vec::new());
    }

    #[test]
    fn it_survives_a_restart_and_never_holds_the_value() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let bytes = std::fs::read(k.file("campus")).expect("the file");
        assert!(!bytes.windows(VALUE.len()).any(|window| window == VALUE.as_bytes()));
        k.reload();
        assert_eq!(k.app.panels.bots.live[0].secrets.find("pw").map(|e| e.value.0.as_str()), Some(VALUE));
    }

    #[test]
    fn a_file_moved_to_another_bot_does_not_open() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        std::fs::copy(k.file("campus"), k.file("prs")).expect("copy");
        k.reload();
        assert_eq!(k.app.panels.bots.live[1].secrets, Kept::Unreadable);
        assert_eq!(k.app.panels.bots.live[0].secrets.count(), 1);
    }

    #[test]
    fn a_file_that_does_not_open_is_never_overwritten_and_only_remove_all_starts_again() {
        let mut k = keeper();
        std::fs::write(k.file("campus"), b"garbage").expect("write");
        k.reload();
        let (code, message) = refusal(k.set("pw", VALUE));
        assert_eq!((code, message.as_str()), (Code::Failed, "the secrets of campus cannot be read"));
        assert_eq!(std::fs::read(k.file("campus")).expect("kept"), b"garbage");
        assert!(refusal(k.app.bot_secret_list(None, "campus")).1.contains("remove campus --all"));
        assert_eq!(refusal(k.app.bot_secret_remove(None, "campus", (Some("pw"), false))).0, Code::Failed);
        let all = k.app.bot_secret_remove(None, "campus", (None, true)).expect("starts again");
        assert_eq!(all["removed"], 0);
        assert!(!k.file("campus").exists());
        assert!(k.set("pw", VALUE).is_ok());
    }

    #[test]
    fn a_key_that_cannot_be_read_refuses_set_and_stays_as_it_is() {
        let mut k = keeper();
        let path = k.app.panels.bots.key_path.clone().expect("a key path");
        std::fs::write(&path, b"short").expect("write");
        k.reload();
        assert_eq!(k.app.panels.bots.key, KeyState::Broken);
        assert!(refusal(k.set("pw", VALUE)).1.contains("cannot be read"));
        assert_eq!(std::fs::read(&path).expect("kept"), b"short");
        assert!(!k.file("campus").exists());
    }

    #[test]
    fn removing_the_last_one_deletes_the_file_and_naming_both_or_neither_is_malformed() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        assert_eq!(refusal(k.app.bot_secret_remove(None, "campus", (None, false))).0, Code::Malformed);
        assert_eq!(refusal(k.app.bot_secret_remove(None, "campus", (Some("pw"), true))).0, Code::Malformed);
        assert_eq!(refusal(k.app.bot_secret_remove(None, "campus", (Some("nope"), false))).0, Code::NotFound);
        k.app.bot_secret_remove(None, "campus", (Some("pw"), false)).expect("removed");
        assert!(!k.file("campus").exists());
    }

    #[test]
    fn removing_the_bot_leaves_nothing_and_renaming_it_keeps_the_secrets() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let edit = crate::control::bots::EditBot {
            bot: String::from("campus"),
            name: Some(String::from("University")),
            ..Default::default()
        };
        k.app.bot_edit(&edit).expect("renamed");
        k.reload();
        assert_eq!(k.app.panels.bots.live[0].secrets.count(), 1);
        k.app.bot_remove("campus").expect("removed");
        assert!(!k.file("campus").exists());
    }
}

#[cfg(unix)]
mod what_is_written {
    use super::super::routines::{START, bot_routine, scheduled};
    use super::*;
    use crate::queue::State;

    fn out_with(value: &str, ending: &str) -> String {
        let step = serde_json::json!({"type": "assistant", "message": {"model": "m", "content": [
            {"type": "tool_use", "name": "Bash", "input": {"command": format!("curl -u me:{value} https://x")}}]}});
        let said = serde_json::json!({"type": "assistant", "message": {"model": "m", "content": [
            {"type": "text", "text": format!("I logged in with {value}.")}]}});
        let result = serde_json::json!({"type": "result", "subtype": "success", "is_error": false, "num_turns": 1,
            "result": format!("I logged in with {value}.")});
        let init = serde_json::json!({"type": "system", "subtype": "init", "model": "m", "session_id": "s-1"});
        format!("{init}\n{step}\n{said}\n{result}\n{ending}")
    }

    fn with_secret(out: &str) -> Chat {
        let mut c = chat(out);
        c.app.bot_secret_set(None, "prs", ("pw", VALUE)).expect("set");
        c
    }

    #[test]
    fn a_step_and_an_answer_that_carry_a_value_are_written_with_its_name() {
        let mut c = with_secret(&out_with(VALUE, ""));
        c.app.bot_say("prs", "log in", false).expect("say");
        c.settle();
        let on_disk = std::fs::read_to_string(c.dir.path().join("prs").join(folder::CHAT_FILE)).expect("chat");
        assert!(!on_disk.contains(VALUE), "{on_disk}");
        let lines = c.lines();
        assert!(lines.iter().any(|line| line.text == "curl -u me:[secret pw] https://x"), "{lines:?}");
        assert!(lines.iter().any(|line| line.text == "I logged in with [secret pw]."), "{lines:?}");
        assert_eq!(
            c.app.panels.bots.live[0].last.as_ref().map(|line| line.text.as_str()),
            Some("I logged in with [secret pw].")
        );
    }

    #[test]
    fn a_value_under_six_bytes_is_left_alone() {
        let mut c = chat(&out_with("abc", ""));
        c.app.bot_secret_set(None, "prs", ("pin", "abc")).expect("set");
        c.app.bot_say("prs", "log in", false).expect("say");
        c.settle();
        assert!(c.lines().iter().any(|line| line.text == "I logged in with abc."));
    }

    #[test]
    fn a_routines_result_is_redacted_too() {
        let mut c = with_secret(&out_with(VALUE, ""));
        let id = scheduled(&mut c, bot_routine(START, "2h"));
        c.fire(START);
        c.runs_settle();
        let run = c.app.panels.queued.queue().runs_of(id).next().cloned().expect("a run");
        assert!(matches!(run.state, State::Answered { .. }), "{:?}", run.state);
        assert_eq!(run.result.as_ref().map(|result| result.text.as_str()), Some("I logged in with [secret pw]."));
        assert!(!c.lines().iter().any(|line| line.text.contains(VALUE)));
    }

    #[test]
    fn an_error_that_carries_a_value_is_redacted() {
        let failed = serde_json::json!({"type": "result", "subtype": "success", "is_error": true, "num_turns": 1,
            "result": format!("login failed for {VALUE}")});
        let mut c = with_secret(&failed.to_string());
        c.app.bot_say("prs", "log in", false).expect("say");
        c.settle();
        let lines = c.lines();
        assert!(lines.iter().any(|line| line.text == "login failed for [secret pw]"), "{lines:?}");
    }
}
