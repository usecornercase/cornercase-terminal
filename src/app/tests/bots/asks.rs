use serde_json::Value;

use super::secrets::{BOUND, Kept2, OWNER, STRANGER, VALUE, keeper, refusal, turn};
use super::*;
use crate::app::overlay::Overlay;
use crate::bots::asks;
use crate::control::bots::{SecretAnswer, SecretAsk, SecretGive};
use crate::control::contract::{Code, Refusal};

type Answer = std::result::Result<Value, Refusal>;

pub(super) fn asking(names: &[&str]) -> SecretAsk {
    SecretAsk {
        names: names.iter().map(|name| (*name).to_string()).collect(),
        about: "to log in".into(),
        replace: false,
    }
}

pub(super) fn giving(bot: &str, ask: u64, pairs: &[(&str, &str)]) -> SecretGive {
    let values = pairs.iter().map(|(name, value)| ((*name).to_string(), Secret((*value).to_string())));
    SecretGive { bot: bot.into(), ask, values: values.collect() }
}

fn ask_id(answer: &Value) -> u64 {
    answer["ask"].as_u64().expect("an ask id")
}

impl Kept2 {
    fn at(&self) -> usize {
        self.app.panels.bots.by_id("campus").expect("campus")
    }

    fn ask(&mut self, names: &[&str]) -> Answer {
        self.app.bot_secret_ask(Some(OWNER), None, &asking(names))
    }

    fn give(&mut self, ask: u64, pairs: &[(&str, &str)]) -> Answer {
        self.app.bot_secret_give(None, &giving("campus", ask, pairs))
    }

    fn waiting(&self) -> Vec<u64> {
        self.app.panels.bots.live[self.at()].asks.iter().map(|ask| ask.id).collect()
    }

    fn chat(&self) -> Vec<Line> {
        chat_of(&self.dir.path().join("bots"), "campus")
    }
}

mod asking {
    use super::*;

    #[test]
    fn writes_a_notice_lists_the_ask_and_answers_at_once() {
        let mut k = keeper();
        let answer = k.ask(&["user", "pw"]).expect("asked");
        assert_eq!((answer["already"].as_bool(), answer["bot"].as_str()), (Some(false), Some("campus")));
        let line = k.chat().pop().expect("a notice");
        assert_eq!(
            (line.kind, line.text.as_str(), line.ask),
            (chat::Kind::Notice, "asks for a secret: user, pw", Some(ask_id(&answer)))
        );
        let info = k.app.bot_info(k.at());
        assert_eq!(info.asks.len(), 1);
        assert_eq!(
            (info.asks[0].names.clone(), info.asks[0].about.as_str(), info.asks[0].routine),
            (vec!["user".to_string(), "pw".to_string()], "to log in", None)
        );
    }

    #[test]
    fn is_the_bots_own_from_its_turn() {
        let mut k = keeper();
        k.app.clients.bots.owned(21, "prs");
        let only = (Code::Failed, asks::ONLY_ITSELF.to_string());
        for client in [Some(STRANGER), Some(BOUND), Some(21), None] {
            assert_eq!(refusal(k.app.bot_secret_ask(client, None, &asking(&["pw"]))), only, "{client:?}");
        }
        let at = k.at();
        k.app.panels.bots.live[at].turn = None;
        assert_eq!(refusal(k.ask(&["pw"])), only);
        assert_eq!(k.chat().len(), 0);
    }

    #[test]
    fn refuses_a_saved_name_unless_it_replaces_it() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let (code, message) = refusal(k.ask(&["pw"]));
        assert_eq!(code, Code::Conflict);
        assert!(message.contains("pw is already saved"), "{message}");
        let replace = SecretAsk { replace: true, ..asking(&["pw"]) };
        assert!(k.app.bot_secret_ask(Some(OWNER), None, &replace).is_ok());
    }

    #[test]
    fn refuses_an_about_that_holds_a_saved_value() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        let about = SecretAsk { about: format!("it was {VALUE}"), ..asking(&["user"]) };
        let (code, message) = refusal(k.app.bot_secret_ask(Some(OWNER), None, &about));
        assert!(code == Code::Malformed && !message.contains(VALUE), "{message}");
    }

    #[test]
    fn refuses_a_name_that_holds_a_saved_value_and_keeps_it_out_of_the_bots_and_the_events() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        k.app.clients.events.subscribe(30, Vec::new(), false, true);
        let refused = refusal(k.ask(&["user", VALUE]));
        assert_eq!(refused, (Code::Malformed, asks::NAME_HOLDS_VALUE.to_string()));
        assert_eq!(k.waiting(), Vec::<u64>::new());
        let infos = serde_json::to_string(&k.app.bot_infos()).expect("json");
        let events = format!("{:?}", k.app.take_events());
        let file = std::fs::read_to_string(k.dir.path().join("bots").join("campus").join(folder::CHAT_FILE))
            .unwrap_or_default();
        for text in [infos, events, file] {
            assert!(!text.contains(VALUE), "{text}");
        }
    }

    #[test]
    fn refuses_an_about_that_holds_a_value_replaced_during_the_turn() {
        let mut k = keeper();
        k.set("pw", VALUE).expect("set");
        k.set("pw", "another-long-value").expect("replaced");
        assert_eq!(k.app.panels.bots.live[k.at()].retired.len(), 1);
        let about = SecretAsk { about: format!("it was {VALUE}"), ..asking(&["user"]) };
        let refused = refusal(k.app.bot_secret_ask(Some(OWNER), None, &about));
        assert_eq!(refused, (Code::Malformed, asks::ABOUT_HOLDS_VALUE.to_string()));
        assert!(k.ask(&["user"]).is_ok());
    }

    #[test]
    fn an_ask_whose_notice_cannot_be_written_is_refused_and_can_be_asked_again() {
        let mut k = keeper();
        let path = k.dir.path().join("bots").join("campus").join(folder::CHAT_FILE);
        let _ = std::fs::remove_file(&path);
        std::fs::create_dir(&path).expect("a folder in the chat's place");
        assert_eq!(refusal(k.ask(&["pw"])).0, Code::Failed);
        assert_eq!(k.waiting(), Vec::<u64>::new());
        std::fs::remove_dir(&path).expect("the folder goes");
        assert_eq!(k.ask(&["pw"]).expect("asked")["already"].as_bool(), Some(false));
        assert_eq!(k.chat().len(), 1);
    }

    #[test]
    fn a_restart_numbers_new_asks_after_those_in_the_chat() {
        let mut k = keeper();
        let old = ask_id(&k.ask(&["pw"]).expect("asked"));
        k.app.panels.bots.next_ask = 1;
        k.reload();
        let at = k.at();
        k.app.panels.bots.live[at].turn = Some(turn());
        let new = ask_id(&k.ask(&["user"]).expect("asked"));
        assert!(new > old, "{new} after {old}");
        assert_eq!(refusal(k.give(old, &[("user", "me")])).0, Code::NotFound);
    }

    #[test]
    fn the_bots_list_what_an_ask_still_misses() {
        let mut k = keeper();
        k.ask(&["user", "pw"]).expect("asked");
        k.set("user", "me").expect("set");
        let info = k.app.bot_info(k.at());
        assert_eq!(info.asks[0].names, ["user", "pw"]);
        assert_eq!(info.asks[0].missing, ["pw"]);
    }

    #[test]
    fn the_same_names_again_are_already_asked_and_a_fifth_ask_is_refused() {
        let mut k = keeper();
        let first = ask_id(&k.ask(&["a", "b"]).expect("asked"));
        let again = k.ask(&["b", "a"]).expect("asked again");
        assert_eq!((again["already"].as_bool(), ask_id(&again)), (Some(true), first));
        assert_eq!(k.chat().len(), 1);
        for name in ["c", "d", "e"] {
            k.ask(&[name]).expect("room");
        }
        assert_eq!(refusal(k.ask(&["f"])).0, Code::Conflict);
        assert_eq!(k.waiting().len(), 4);
    }
}

mod giving {
    use super::*;

    #[test]
    fn saves_every_name_or_none() {
        let mut k = keeper();
        let ask = ask_id(&k.ask(&["user", "pw"]).expect("asked"));
        let long = "x".repeat(8193);
        assert_eq!(refusal(k.give(ask, &[("user", "me"), ("pw", &long)])).0, Code::Malformed);
        assert_eq!(k.app.panels.bots.live[k.at()].secrets.count(), 0);
        assert_eq!(k.waiting(), [ask]);
        let saved = k.give(ask, &[("user", "me"), ("pw", VALUE)]).expect("given");
        assert_eq!(saved["saved"], serde_json::json!(["pw", "user"]));
        assert_eq!(k.app.panels.bots.live[k.at()].secrets.count(), 2);
        assert_eq!(k.waiting(), Vec::<u64>::new());
    }

    #[test]
    fn refuses_a_name_not_asked_a_missing_one_an_unknown_ask_and_a_bot() {
        let mut k = keeper();
        let ask = ask_id(&k.ask(&["user", "pw"]).expect("asked"));
        assert_eq!(refusal(k.give(ask, &[("user", "me"), ("pw", VALUE), ("other", "x")])).0, Code::Malformed);
        let (code, message) = refusal(k.give(ask, &[("user", "me")]));
        assert!(code == Code::Malformed && message.contains("user, pw"), "{message}");
        assert_eq!(refusal(k.give(ask + 9, &[("user", "me")])).0, Code::NotFound);
        let by_bot = k.app.bot_secret_give(Some(OWNER), &giving("campus", ask, &[("user", "me"), ("pw", VALUE)]));
        assert_eq!(refusal(by_bot), (Code::Failed, asks::NO_ANSWER.to_string()));
        let declined = k.app.bot_secret_decline(Some(BOUND), &SecretAnswer { bot: "campus".into(), ask });
        assert_eq!(refusal(declined).1, asks::NO_ANSWER);
        assert_eq!(k.waiting(), [ask]);
    }

    #[test]
    fn a_set_of_the_last_missing_name_ends_the_ask_and_a_removal_makes_it_missing_again() {
        let mut k = keeper();
        let ask = ask_id(&k.ask(&["user", "pw"]).expect("asked"));
        k.set("user", "me").expect("set");
        k.app.bot_secret_remove(None, "campus", (Some("user"), false)).expect("removed");
        k.set("pw", VALUE).expect("set");
        assert_eq!(k.waiting(), [ask], "user was removed, so it is missing again");
        k.set("user", "me").expect("set");
        assert_eq!(k.waiting(), Vec::<u64>::new());
        assert!(texts(&k.chat()).contains(&"secret saved: user, pw"));
    }

    #[test]
    fn declining_writes_its_notice_and_tells_the_chat_later() {
        let mut k = keeper();
        let ask = ask_id(&k.ask(&["pw"]).expect("asked"));
        k.app.bot_secret_decline(None, &SecretAnswer { bot: "campus".into(), ask }).expect("declined");
        assert_eq!(texts(&k.chat()).last(), Some(&"secret not given: pw"));
        assert_eq!(k.app.panels.bots.live[k.at()].notes, ["The user did not give: pw."]);
        let again = k.app.bot_secret_decline(None, &SecretAnswer { bot: "campus".into(), ask });
        assert_eq!(refusal(again).0, Code::NotFound);
    }
}

mod ending {
    use super::*;

    #[test]
    fn removing_the_bot_drops_its_asks_and_closes_the_dialog() {
        let mut k = keeper();
        let ask = ask_id(&k.ask(&["pw"]).expect("asked"));
        let key = k.app.panels.bots.live[k.at()].key;
        k.app.open_bot_ask(key, ask);
        assert!(matches!(k.app.overlay, Some(Overlay::BotAsk(_))));
        let at = k.at();
        k.app.panels.bots.live[at].turn = None;
        k.app.bot_remove("campus").expect("removed");
        assert!(k.app.overlay.is_none());
    }

    #[test]
    fn a_restart_forgets_the_asks_but_still_tells_what_was_declined() {
        let mut k = keeper();
        let ask = ask_id(&k.ask(&["pw"]).expect("asked"));
        k.ask(&["user"]).expect("asked");
        k.app.bot_secret_decline(None, &SecretAnswer { bot: "campus".into(), ask }).expect("declined");
        k.reload();
        assert_eq!(k.waiting(), Vec::<u64>::new());
        assert_eq!(k.app.panels.bots.live[k.at()].notes, ["The user did not give: pw."]);
    }

    #[test]
    fn no_value_given_is_in_the_chat_the_events_or_the_bots() {
        let mut k = keeper();
        k.app.clients.events.subscribe(30, Vec::new(), false, true);
        let ask = ask_id(&k.ask(&["pw"]).expect("asked"));
        k.give(ask, &[("pw", VALUE)]).expect("given");
        let file =
            std::fs::read_to_string(k.dir.path().join("bots").join("campus").join(folder::CHAT_FILE)).expect("chat");
        let events = format!("{:?}", k.app.take_events());
        let infos = serde_json::to_string(&k.app.bot_infos()).expect("json");
        for text in [file, events, infos] {
            assert!(!text.contains(VALUE), "{text}");
        }
    }
}

mod turns {
    use super::*;

    fn running(c: &mut Chat) {
        c.hold();
        c.say("first");
        c.started(1);
        c.app.clients.bots.owned(OWNER, "prs");
    }

    fn ask(c: &mut Chat, run: Option<u64>) -> u64 {
        ask_id(&c.app.bot_secret_ask(Some(OWNER), run, &asking(&["pw"])).expect("asked"))
    }

    #[test]
    fn a_chat_ask_saved_starts_a_turn_that_says_so() {
        let mut c = chat(ANSWER);
        running(&mut c);
        let ask = ask(&mut c, None);
        c.release(ANSWER);
        c.settle();
        c.app.bot_secret_give(None, &giving("prs", ask, &[("pw", VALUE)])).expect("given");
        let saved = c.lines().into_iter().find(|line| line.text == "secret saved: pw").expect("the notice");
        assert_eq!((saved.ask, saved.held, saved.routine), (Some(ask), false, None));
        assert!(c.app.panels.bots.live[0].turn.is_some());
        c.started(2);
        c.settle();
        let argvs = c.argvs();
        assert!(argvs[1].contains("The user saved the secrets you asked for: pw."), "{}", argvs[1]);
        assert!(argvs[1].contains("cornercase says"), "{}", argvs[1]);
        assert!(!argvs[1].contains(VALUE), "{}", argvs[1]);
    }

    #[test]
    fn saved_while_a_turn_runs_it_is_held_and_goes_next() {
        let mut c = chat(ANSWER);
        running(&mut c);
        let ask = ask(&mut c, None);
        c.app.bot_secret_give(None, &giving("prs", ask, &[("pw", VALUE)])).expect("given");
        let saved = c.lines().into_iter().find(|line| line.text == "secret saved: pw").expect("the notice");
        assert!(saved.held);
        assert_eq!(c.app.panels.bots.live[0].held.len(), 1);
        c.release(ANSWER);
        c.started(2);
        c.settle();
        assert!(c.argvs()[1].contains("The user saved the secrets you asked for: pw."), "{:?}", c.argvs());
    }

    #[test]
    fn a_routines_ask_starts_nothing_and_the_next_message_is_told() {
        let mut c = chat(ANSWER);
        running(&mut c);
        let ask = ask(&mut c, Some(41));
        c.release(ANSWER);
        c.settle();
        let notice = c.lines().into_iter().find(|line| line.ask == Some(ask)).expect("the ask");
        assert_eq!(notice.routine, Some(41));
        c.app.bot_secret_give(None, &giving("prs", ask, &[("pw", VALUE)])).expect("given");
        assert!(c.app.panels.bots.live[0].turn.is_none());
        let saved = c.lines().into_iter().find(|line| line.text == "secret saved: pw").expect("the notice");
        assert_eq!(saved.routine, Some(41));
        c.say("next");
        c.settle();
        assert!(c.argvs()[1].contains("The user saved the secrets you asked for: pw."), "{:?}", c.argvs());
    }

    #[test]
    fn the_chat_asking_what_a_routine_asked_is_carried_on_in_the_chat_once_saved() {
        let mut c = chat(ANSWER);
        running(&mut c);
        let first = ask(&mut c, Some(41));
        assert_eq!(ask(&mut c, None), first);
        assert_eq!(c.app.panels.bots.live[0].asks[0].run, None);
        c.release(ANSWER);
        c.settle();
        c.app.bot_secret_give(None, &giving("prs", first, &[("pw", VALUE)])).expect("given");
        let saved = c.lines().into_iter().find(|line| line.text == "secret saved: pw").expect("the notice");
        assert_eq!(saved.routine, None);
        assert!(c.app.panels.bots.live[0].turn.is_some());
    }

    #[test]
    fn a_routine_asking_what_the_chat_asked_leaves_it_the_chats() {
        let mut c = chat(ANSWER);
        running(&mut c);
        let first = ask(&mut c, None);
        assert_eq!(ask(&mut c, Some(41)), first);
        assert_eq!(c.app.panels.bots.live[0].asks[0].run, None);
    }

    #[test]
    fn a_decline_is_told_once_at_the_next_message() {
        let mut c = chat(ANSWER);
        running(&mut c);
        let ask = ask(&mut c, None);
        c.release(ANSWER);
        c.settle();
        c.app.bot_secret_decline(None, &SecretAnswer { bot: "prs".into(), ask }).expect("declined");
        assert!(c.app.panels.bots.live[0].turn.is_none());
        c.say("next");
        c.settle();
        c.say("again");
        c.settle();
        let argvs = c.argvs();
        assert!(argvs[1].contains("The user did not give: pw."), "{}", argvs[1]);
        assert!(!argvs[2].contains("did not give"), "{}", argvs[2]);
    }

    #[test]
    fn an_error_after_a_saved_turn_offers_no_retry() {
        let mut c = chat(ANSWER);
        running(&mut c);
        let ask = ask(&mut c, None);
        c.release(ANSWER);
        c.settle();
        std::fs::write(c.claude.with_extension("after"), FAILED).expect("a failure");
        c.app.bot_secret_give(None, &giving("prs", ask, &[("pw", VALUE)])).expect("given");
        c.settle();
        assert_eq!(c.kinds().last(), Some(&chat::Kind::Error));
        c.app.show_bot(0);
        render(&mut c.app, AREA);
        let shown = c.app.shown_chat(AREA).expect("a chat");
        let retry = shown.view.rows.iter().any(|row| matches!(row.hit, Some((RowHit::Retry(_), _))));
        assert!(!retry);
    }
}
