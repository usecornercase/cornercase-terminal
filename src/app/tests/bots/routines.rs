use super::*;
use crate::queue::repeat::Repeat;
use crate::queue::{self, Place, State, Task};

pub(super) const START: i64 = 1_800_000_000;

pub(super) fn bot_routine(at: i64, every: &str) -> Task {
    Task {
        at,
        zone: "UTC".into(),
        label: "Review PRs".into(),
        agent: "claude".into(),
        prompt: "Check my pull requests.".into(),
        submit: true,
        place: Place { bot: Some("prs".into()), ..Place::default() },
        repeat: Some(Repeat::parse(every, "UTC", at).expect("repeat")),
        ..Task::default()
    }
}

pub(super) fn scheduled(c: &mut Chat, task: Task) -> u64 {
    c.app.set_queue_area(AREA);
    c.app.panels.queued.schedule(None, task).expect("scheduled")
}

fn runs(c: &Chat, routine: u64) -> Vec<queue::Entry> {
    c.app.panels.queued.queue().runs_of(routine).cloned().collect()
}

impl Chat {
    pub(super) fn runs_settle(&mut self) {
        let Self { app, rx, .. } = self;
        wait_until("the run ends", || {
            drain(app, rx);
            app.panels.bots.live[0].runs.is_empty()
                && !app.panels.queued.queue().entries.iter().any(|entry| entry.state == State::Starting)
        });
    }

    fn fire(&mut self, at: i64) {
        self.app.refresh_at(Instant::now(), at);
    }

    fn started_run(&mut self, runs: usize) {
        self.started(runs);
    }
}

fn routine_lines(lines: &[Line]) -> Vec<&Line> {
    lines.iter().filter(|line| line.routine.is_some()).collect()
}

#[test]
fn a_due_bot_routine_answers_in_its_chat_as_a_block_and_ends_answered() {
    use chat::Kind::{Bot, Notice, Step};
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.runs_settle();
    let run = runs(&c, id).remove(0);
    let lines = c.lines();
    assert_eq!(lines.iter().map(|line| line.kind).collect::<Vec<_>>(), [Notice, Step, Step, Bot]);
    assert!(lines.iter().all(|line| line.routine == Some(run.id)), "{lines:?}");
    assert!(lines[0].text.starts_with("↻ Review PRs · "), "{}", lines[0].text);
    assert_eq!(lines[3].text, "Two are blocked.");
    let State::Answered { session } = &run.state else { panic!("{:?}", run.state) };
    assert_eq!(session.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
    assert_eq!(run.result.as_ref().map(|result| result.text.as_str()), Some("Two are blocked."));
    assert_eq!(c.app.panels.bots.live[0].bot.conversation, None);
    assert_eq!(c.app.panels.queued.queue().entry(id).expect("routine").task.at, START + 7200);
    the_run_opened_a_new_session(&c);
}

fn the_run_opened_a_new_session(c: &Chat) {
    let argv = &c.argvs()[0];
    assert!(argv.contains(r#"This is your routine \"Review PRs\" (every 2h)"#), "{argv}");
    assert!(!argv.contains("--resume"), "{argv}");
}

#[test]
fn a_runs_answer_is_unread_and_notifies_with_the_routines_name() {
    let mut c = chat(ANSWER);
    scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.runs_settle();
    assert_eq!(c.app.panels.bots.live[0].unread, 1);
    let toast = c.app.messages.toast.as_ref().expect("a toast");
    assert_eq!(toast.message, "PRs · Review PRs: Two are blocked.");
    let token = toast.go.clone().expect("a go");
    assert_eq!(c.app.outbox.take_notifications().len(), 1);
    c.app.notification_clicked(&token);
    assert_eq!(c.app.shown_bot(), Some(0));
    assert!(c.app.panels.bots.shown.as_ref().is_some_and(|shown| shown.anchor.is_some() || shown.scroll == 0));
}

#[test]
fn an_answer_on_screen_is_neither_unread_nor_notified() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    c.app.show_bot(0);
    scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.runs_settle();
    assert_eq!(c.app.panels.bots.live[0].unread, 0);
    assert!(c.app.messages.toast.is_none());
    assert_eq!(c.app.outbox.take_notifications().len(), 0);
}

#[test]
fn missed_paused_and_start_now_work_for_a_bot_routine() {
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START + 7 * 86_400 + 3_600);
    let found = runs(&c, id);
    assert!(found.iter().all(|run| run.state == State::Missed), "{found:?}");
    assert_eq!(c.argvs(), Vec::<String>::new());
    c.app.panels.queued.pause(id, true, START + 7 * 86_400).expect("pause");
    assert_eq!(c.app.panels.queued.queue().next(START + 7 * 86_400 + 1_000_000), None);
    c.app.panels.queued.pause(id, false, START + 7 * 86_400).expect("resume");
    let at = c.app.panels.queued.queue().entry(id).expect("routine").task.at;
    c.app.run_queued(id, Instant::now()).expect("start now");
    c.runs_settle();
    let answered: Vec<_> = runs(&c, id).into_iter().filter(|run| matches!(run.state, State::Answered { .. })).collect();
    assert_eq!(answered.len(), 1);
    assert_eq!(c.app.panels.queued.queue().entry(id).expect("routine").task.at, at);
}

#[test]
fn a_run_fails_visibly_when_its_agent_or_its_account_is_missing_or_its_bot_is_gone() {
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.app.config.agent_commands.insert("claude".into(), "/nonexistent/claude".into());
    c.fire(START);
    let run = runs(&c, id).remove(0);
    let State::Failed { reason } = &run.state else { panic!("{:?}", run.state) };
    assert_eq!(reason, "claude is not installed");
    let lines = c.lines();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].text.starts_with("↻ Review PRs"), "{lines:?}");
    assert_eq!((lines[1].kind, lines[1].routine), (chat::Kind::Error, Some(run.id)));
    assert_eq!(
        c.app.messages.toast.as_ref().map(|toast| toast.message.as_str()),
        Some("PRs · Review PRs failed: claude is not installed")
    );
    assert_eq!(c.app.panels.bots.live[0].unread, 1);

    let ghost =
        Task { place: Place { bot: Some("ghost".into()), ..Place::default() }, ..bot_routine(START + 9_000, "2h") };
    let ghost = scheduled(&mut c, ghost);
    c.fire(START + 9_000);
    let run = runs(&c, ghost).remove(0);
    let State::Failed { reason } = &run.state else { panic!("{:?}", run.state) };
    assert_eq!(reason, "its bot ghost was removed");
    assert!(c.app.messages.toast.as_ref().is_some_and(|toast| toast.message.contains("Review PRs failed")));
}

#[test]
fn a_run_and_a_chat_turn_answer_at_the_same_time_in_two_sessions() {
    let mut c = chat(ANSWER);
    c.hold();
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.started(1);
    c.say("and the chat?");
    c.started_run(2);
    c.release(ANSWER);
    c.runs_settle();
    c.settle();
    let argvs = c.argvs();
    assert_eq!(argvs.len(), 2);
    assert!(argvs.iter().any(|argv| argv.contains("This is your routine")));
    assert!(argvs.iter().any(|argv| argv.contains("and the chat?")));
    let lines = c.lines();
    let run = runs(&c, id).remove(0).id;
    assert!(routine_lines(&lines).iter().all(|line| line.routine == Some(run)));
    assert!(lines.iter().any(|line| line.kind == chat::Kind::You && line.routine.is_none()));
    assert_eq!(lines.iter().filter(|line| line.kind == chat::Kind::Bot).count(), 2);
}

#[test]
fn routine_answers_go_in_front_of_the_next_message_once() {
    let mut c = chat(ANSWER);
    scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.runs_settle();
    c.say("ok, thanks");
    c.settle();
    let argvs = c.argvs();
    assert!(argvs[1].contains("Since your last message, your routines answered"), "{}", argvs[1]);
    assert!(argvs[1].contains("[Review PRs, "), "{}", argvs[1]);
    assert!(argvs[1].contains("Two are blocked."));
    c.say("and now?");
    c.settle();
    assert!(!c.argvs()[2].contains("Since your last message"));
    c.app.set_bots_dir(c.dir.path());
    assert!(c.app.panels.bots.live[0].unseen.is_empty(), "the answer came before the last message");
}

#[test]
fn an_answer_written_after_the_last_message_is_told_once_after_a_reload() {
    let mut c = chat(ANSWER);
    scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.runs_settle();
    c.app.set_bots_dir(c.dir.path());
    assert_eq!(c.app.panels.bots.live[0].unseen.len(), 1);
    c.say("hi");
    c.settle();
    assert!(c.argvs().last().is_some_and(|argv| argv.contains("Since your last message")));
    assert!(c.app.panels.bots.live[0].unseen.is_empty());
}

#[test]
fn removing_a_bot_removes_its_entries_in_one_undo_and_is_refused_while_a_run_answers() {
    let mut c = chat(ANSWER);
    c.hold();
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.started(1);
    assert!(c.app.bot_remove("prs").is_err());
    c.release(ANSWER);
    c.runs_settle();
    let (bot, entries) = c.app.bot_remove("prs").expect("removed");
    assert_eq!(bot, "prs");
    assert_eq!(entries.len(), 2);
    assert_eq!(c.app.panels.queued.queue().entries, Vec::<queue::Entry>::new());
    let undo = c.app.panels.queued.keep_removed(entries).expect("an undo");
    let back = c.app.panels.queued.take_removed(undo).expect("the entries");
    assert!(back.iter().any(|entry| entry.id == id));
}

#[test]
fn stopping_a_bot_stops_its_runs() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.timeout = Duration::from_secs(600);
    c.hold();
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.fire(START);
    c.started(1);
    assert_eq!(c.app.bot_stop("prs"), Ok(true));
    c.runs_settle();
    let run = runs(&c, id).remove(0);
    assert_eq!(run.state, State::Failed { reason: "stopped".into() });
    let lines = c.lines();
    let last = lines.last().expect("a line");
    assert_eq!((last.text.as_str(), last.routine), ("stopped", Some(run.id)));
    assert!(c.app.messages.toast.is_none());
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
}

#[test]
fn a_run_left_by_a_restart_is_closed_in_the_chat() {
    let mut c = chat(ANSWER);
    let path = folder::of(c.dir.path(), "prs").join(folder::CHAT_FILE);
    chat::append(&path, &Line::notice(1, 5, "↻ Review PRs · 09:00").of_run(12)).expect("append");
    chat::append(&path, &Line::step(2, 6, "Bash", "ls").of_run(12)).expect("append");
    c.app.set_bots_dir(c.dir.path());
    c.app.set_bots_dir(c.dir.path());
    let lines = c.lines();
    assert_eq!(lines.len(), 3);
    assert_eq!((lines[2].text.as_str(), lines[2].routine), ("interrupted by a restart", Some(12)));
}

fn answered_run(c: &mut Chat) -> (u64, u64) {
    let id = scheduled(c, bot_routine(START, "2h"));
    c.fire(START);
    c.runs_settle();
    (id, runs(c, id).remove(0).id)
}

#[test]
fn enter_on_an_answered_row_shows_the_chat_at_that_run() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    let (_, run) = answered_run(&mut c);
    c.app.open_queued(run, AREA);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.shown_bot(), Some(0));
    let anchor = c.app.panels.bots.shown.as_ref().and_then(|shown| shown.anchor);
    let first = c.lines().iter().find(|line| line.routine == Some(run)).map(|line| line.id);
    assert_eq!(anchor, first);
}

#[test]
fn go_on_a_runs_toast_shows_the_chat_at_that_run() {
    let mut c = chat(ANSWER);
    let (_, run) = answered_run(&mut c);
    let token = c.app.messages.toast.as_ref().and_then(|toast| toast.go.clone()).expect("a go");
    c.app.notification_clicked(&token);
    let first = c.lines().iter().find(|line| line.routine == Some(run)).map(|line| line.id);
    assert_eq!(c.app.panels.bots.shown.as_ref().and_then(|shown| shown.anchor), first);
}

#[test]
fn an_idle_bot_shows_its_next_run_and_a_running_one_does_not() {
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(crate::time::now() + 86_400, "2h"));
    let next = c.app.panels.queued.queue().entry(id).expect("routine").task.at;
    let shown = queue::clock::short(next, crate::time::now(), &c.app.window_zone());
    assert_eq!(c.app.bots_view().expect("the section").entries[0].detail, format!("next {shown}"));
    c.hold();
    c.say("go");
    c.started(1);
    assert_eq!(c.app.bots_view().expect("the section").entries[0].detail, "claude");
    assert!(c.app.bots_view().expect("the section").entries[0].thinking);
    c.release(ANSWER);
    c.settle();
    let card = c.app.bot_card(0);
    assert!(card.rows.iter().any(|row| row.label == "routine" && row.value.contains("Review PRs")), "{card:?}");
}

#[test]
fn the_remove_dialog_names_the_bots_entries() {
    let mut c = chat(ANSWER);
    scheduled(&mut c, bot_routine(START, "2h"));
    let key = c.app.panels.bots.live[0].key;
    let view = c.app.remove_bot_view(key).expect("the dialog");
    let ui::Overlay::Confirm(confirm) = view else { panic!("a confirmation") };
    assert!(confirm.message.contains("its queue entries (Review PRs) are deleted"), "{}", confirm.message);
}

#[test]
fn a_run_fails_visibly_when_the_bots_account_is_gone() {
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.app.panels.bots.live[0].bot.account = Some("gone".into());
    c.fire(START);
    let run = runs(&c, id).remove(0);
    assert_eq!(run.state, State::Failed { reason: "there is no claude account `gone`".into() });
    let lines = c.lines();
    assert_eq!((lines[1].kind, lines[1].routine), (chat::Kind::Error, Some(run.id)));
    let toast = c.app.messages.toast.as_ref().map(|toast| toast.message.as_str());
    assert_eq!(toast, Some("PRs · Review PRs failed: there is no claude account `gone`"));
}

#[test]
fn queue_run_on_an_answered_entry_is_refused_and_leaves_it_as_it_was() {
    let mut c = chat(ANSWER);
    let (_, run) = answered_run(&mut c);
    let before = c.app.panels.queued.queue().entry(run).expect("run").clone();
    let lines = c.lines().len();
    let refusal = c.app.run_queued(run, Instant::now()).expect_err("refused");
    assert_eq!(refusal, "this queue entry has already started");
    assert_eq!(c.app.panels.queued.queue().entry(run), Some(&before));
    assert_eq!(c.lines().len(), lines);
}

#[test]
fn editing_a_bots_entry_offers_only_that_bot_in_where() {
    let mut c = chat(ANSWER);
    let docs = Bot { access: Access::Cornercase, ..bot("docs", "Docs") };
    folder::create(c.dir.path(), &docs, "").expect("a second bot");
    c.app.set_bots_dir(c.dir.path());
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    c.app.new_queue_form(None);
    let Some(Overlay::Queue(form)) = &c.app.overlay else { panic!("the form") };
    assert_eq!(form.places.iter().map(|choice| choice.label.as_str()).collect::<Vec<_>>(), ["bot: Docs", "bot: PRs"]);
    c.app.overlay = None;
    c.app.new_queue_form(Some(id));
    let Some(Overlay::Queue(form)) = &c.app.overlay else { panic!("the form") };
    assert_eq!(form.places.iter().map(|choice| choice.label.as_str()).collect::<Vec<_>>(), ["bot: PRs"]);
    assert_eq!(form.bot(), Some("prs"));
}
