use std::time::{Duration, Instant};

use super::{BotEvent, News, Starting, Thread, Turn};
use crate::agents::Known;
use crate::agents::headless::{Ran, Watch};
use crate::agents::probe::Running;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::bots::brief::Told;
use crate::bots::chat::{self, Answer, Kind, Line};
use crate::bots::turn::Session;
use crate::bots::{brief, folder, turn};
use crate::control::contract::{Code, Refusal};
use crate::error::Result;
use crate::log::{self, Job, Level};

const ANSWERS_KEPT: usize = 32;
const TOAST_WIDTH: usize = 100;

type Done<T> = std::result::Result<T, Refusal>;

impl App {
    pub(in crate::app) fn bot_at(&self, key: &str) -> Done<usize> {
        let bots: Vec<&crate::bots::Bot> = self.panels.bots.live.iter().map(|state| &state.bot).collect();
        crate::bots::find(&bots, key)
            .and_then(|bot| self.panels.bots.by_id(&bot.id))
            .ok_or_else(|| Refusal::new(Code::NotFound, format!("no bot {key}; cornercase bot list shows them")))
    }

    pub(super) fn bot_append(&mut self, at: usize, make: impl FnOnce(u64, i64) -> Line) -> Line {
        let state = &mut self.panels.bots.live[at];
        let line = make(state.next_line, crate::time::now());
        state.next_line += 1;
        let _ = chat::append(&state.folder.join(folder::CHAT_FILE), &line);
        if matches!(line.kind, Kind::You | Kind::Bot) {
            state.last = Some(line.clone());
        }
        let id = state.bot.id.clone();
        self.chat_took(at, &line);
        self.publish_bot(&id, News::Line(Box::new(line.clone())));
        line
    }

    pub(in crate::app) fn bot_say(&mut self, key: &str, text: &str, wait: bool) -> Done<(usize, Line)> {
        let at = self.bot_at(key)?;
        if let Some(why) = crate::bots::message_refusal(text) {
            return Err(Refusal::new(Code::Malformed, why));
        }
        let busy = self.panels.bots.live[at].turn.is_some();
        if !busy {
            let id = self.panels.bots.live[at].bot.id.clone();
            self.publish_bot(&id, News::State("thinking"));
        }
        let line = self.bot_append(at, |id, now| Line::you(id, now, text, busy));
        let state = &mut self.panels.bots.live[at];
        if wait {
            state.waited.insert(line.id);
        }
        if busy {
            state.held.push((line.id, text.to_string()));
        } else {
            self.bot_start(at, &[(line.id, text.to_string())], Starting::chat());
        }
        Ok((at, line))
    }

    pub(super) fn bot_input(&self, at: usize, text: String) -> turn::Input {
        let state = &self.panels.bots.live[at];
        let kind = Known::of(&state.bot.agent);
        turn::Input {
            bot: state.bot.clone(),
            folder: state.folder.clone(),
            text,
            session: Session::Chat,
            from_cornercase: false,
            told: None,
            program: crate::agents::command(&self.config, &state.bot.agent),
            home: self.account_home(kind, state.bot.account.as_deref()),
            socket: self.shells.pane_socket().map(str::to_string),
            timeout: self.panels.bots.timeout,
        }
    }

    pub(super) fn bot_start(&mut self, at: usize, asked: &[(u64, String)], how: Starting) {
        let texts: Vec<String> = asked.iter().map(|(_, text)| text.clone()).collect();
        let mut input = self.bot_input(at, turn::joined(&texts));
        let thread = match &how.session {
            Session::Resume { run, .. } => Thread::Run(*run),
            Session::Chat | Session::NewRun(_) => Thread::Chat,
        };
        if thread == Thread::Chat {
            let told = std::mem::take(&mut self.panels.bots.live[at].unseen);
            input.told = brief::told(&told, &input.bot.id);
        }
        input.session = how.session;
        input.from_cornercase = how.cornercase;
        let (id, running) = (self.panels.bots.next_turn, Running::default());
        self.panels.bots.next_turn += 1;
        let state = &mut self.panels.bots.live[at];
        state.turn = Some(Turn {
            id,
            thread,
            asked: asked.iter().map(|(line, _)| *line).collect(),
            running: running.clone(),
            stopping: false,
            dropped: Vec::new(),
            started: Instant::now(),
            steps: 0,
        });
        self.spawn_bot_job(input, id, running);
    }

    pub(super) fn spawn_bot_job(&mut self, input: turn::Input, turn: u64, running: Running) {
        let bot = input.bot.id.clone();
        let job = Job::new(Level::Info, "bots", "turn").with("bot", &bot).with("agent", &input.bot.agent);
        let tx = self.tx.clone();
        let (steps_bot, ended_bot, started) = (bot.clone(), bot, Instant::now());
        let steps = Box::new(move |step| {
            let event = BotEvent::Step { bot: steps_bot.clone(), turn, step };
            let _ = tx.send(AppEvent::Bot(Box::new(event)));
        });
        let watch = Watch { steps: Some(steps), running: Some(running), ..Watch::default() };
        self.spawn_job(
            job,
            move || {
                let ran = turn::run(&input, watch);
                if let Ok(ran) = &ran {
                    crate::highlight::markdown::warm(&ran.text);
                }
                Ok(ran)
            },
            move |result, _| {
                let (result, took) = (result.and_then(|inner| inner), started.elapsed());
                Some(AppEvent::Bot(Box::new(BotEvent::Ended { bot: ended_bot, turn, result, took })))
            },
        );
    }

    pub(in crate::app) fn bot_event(&mut self, event: BotEvent) {
        match event {
            BotEvent::Step { bot, turn, step } => self.bot_step(&bot, turn, &step),
            BotEvent::Read { bot, read } => self.bot_report_read(&bot, *read),
            BotEvent::Ended { bot, turn, result, took } => {
                if self.running_run(&bot, turn).is_some() {
                    self.run_ended(&bot, turn, result, took);
                } else {
                    self.bot_ended(&bot, turn, result, took);
                }
            }
        }
    }

    pub(super) fn running_run(&self, bot: &str, turn: u64) -> Option<(usize, usize)> {
        let at = self.panels.bots.by_id(bot)?;
        let run = self.panels.bots.live[at].runs.iter().position(|run| run.turn == turn)?;
        Some((at, run))
    }

    fn running_turn(&self, bot: &str, turn: u64) -> Option<usize> {
        let at = self.panels.bots.by_id(bot)?;
        self.panels.bots.live[at].turn.as_ref().is_some_and(|running| running.id == turn).then_some(at)
    }

    fn bot_step(&mut self, bot: &str, turn: u64, step: &crate::agents::headless::Action) {
        if let Some((at, run)) = self.running_run(bot, turn) {
            self.run_step(at, run, step);
        } else if let Some(at) = self.running_turn(bot, turn) {
            if let Some(running) = self.panels.bots.live[at].turn.as_mut() {
                running.steps += 1;
            }
            self.bot_append(at, |id, now| Line::step(id, now, &step.tool, &step.text));
        }
    }

    fn bot_outcome(&mut self, at: usize, result: Result<Ran>, how: (Thread, bool)) -> (Line, &'static str) {
        let (thread, stopping) = how;
        let run = match thread {
            Thread::Run(run) => Some(run),
            Thread::Chat => None,
        };
        let agent = self.panels.bots.live[at].bot.agent.clone();
        match result {
            Ok(ran) => {
                let state = &mut self.panels.bots.live[at];
                if run.is_none() && ran.session.is_some() {
                    state.bot.conversation.clone_from(&ran.session);
                    let _ = folder::save(&state.folder, &state.bot);
                }
                let model = ran.model.clone().or_else(|| state.bot.model.clone());
                let answer = Answer { model, tokens: ran.tokens, conversation: ran.session };
                let line = self.bot_append(at, |id, now| Line::answer(id, now, &ran.text, answer).in_run(run));
                if let Some(run) = run {
                    self.told_of_run(at, run, &line);
                }
                (line, "answered")
            }
            Err(_) if stopping => (self.bot_append(at, |id, now| Line::notice(id, now, chat::STOPPED)), "stopped"),
            Err(error) => {
                let text = turn::failure(&agent, &error, &self.panels.bots.live[at].bot.id);
                let how = if text.contains("did not answer in time") { "timeout" } else { "error" };
                (self.bot_append(at, |id, now| Line::failed(id, now, &text).in_run(run)), how)
            }
        }
    }

    fn bot_ended(&mut self, bot: &str, turn: u64, result: Result<Ran>, took: Duration) {
        let Some(at) = self.running_turn(bot, turn) else { return };
        let Some(finished) = self.panels.bots.live[at].turn.take() else { return };
        let tokens = result.as_ref().map_or(0, |ran| ran.tokens.total());
        let (line, how) = self.bot_outcome(at, result, (finished.thread, finished.stopping));
        let dropped = self.bot_dropped(at, &finished);
        let on_screen = self.bot_on_screen(at);
        let state = &mut self.panels.bots.live[at];
        let shown = finished.asked.iter().any(|id| state.waited.contains(id)) || on_screen;
        self.bot_answer_waiters(at, &finished.asked, &line);
        if let Some(notice) = dropped {
            self.bot_answer_waiters(at, &finished.dropped, &notice);
        }
        if !shown && matches!(line.kind, Kind::Bot | Kind::Error) {
            let who = self.panels.bots.live[at].bot.name.clone();
            self.bot_notify(at, &line, &who);
        }
        let state = &self.panels.bots.live[at];
        log::info!(
            "bots",
            "turn",
            bot = state.bot.id,
            agent = state.bot.agent,
            model = line.model.clone().unwrap_or_default(),
            tokens = tokens,
            ms = took.as_millis(),
            ended = how
        );
        self.bot_next(at);
    }

    fn told_of_run(&mut self, at: usize, run: u64, line: &Line) {
        let (label, clock) = self.run_label(run, line.at);
        self.tell_chat(at, Told { label, clock, text: line.text.clone() });
    }

    fn bot_dropped(&mut self, at: usize, finished: &Turn) -> Option<Line> {
        let count = finished.dropped.len();
        (count > 0).then(|| self.bot_append(at, |id, now| Line::notice(id, now, &chat::dropped(count))))
    }

    fn bot_answer_waiters(&mut self, at: usize, asked: &[u64], line: &Line) {
        let state = &mut self.panels.bots.live[at];
        for id in asked {
            if state.waited.remove(id) {
                state.answers.push_back((*id, line.clone()));
            }
        }
        while state.answers.len() > ANSWERS_KEPT {
            state.answers.pop_front();
        }
    }

    pub(super) fn bot_notify(&mut self, at: usize, line: &Line, who: &str) {
        let state = &mut self.panels.bots.live[at];
        state.unread += 1;
        let bot = state.bot.id.clone();
        let first = line.text.lines().find(|text| !text.trim().is_empty()).unwrap_or_default();
        let text = crate::text::truncate_right(&crate::text::one_line(first), TOAST_WIDTH);
        let message = crate::host::notify::clean(&format!("{who}: {text}"));
        let status = if line.kind == Kind::Error {
            crate::agents::activity::Status::Failed
        } else {
            crate::agents::activity::Status::Done
        };
        let token = self.messages.notices.bot_token(&bot, line.routine);
        let icon = crate::ui::ToastIcon::Agent(status);
        self.messages.toast = Some(super::super::toast::Toast {
            go: Some(token.clone()),
            ..super::super::toast::Toast::new(message.clone(), icon)
        });
        if let Some(note) = crate::host::notify::Notification::new(&message, &self.config.desktop_notifications) {
            self.outbox.notify(Some(crate::host::notify::Notification { token: Some(token), ..note }));
        }
    }

    fn bot_next(&mut self, at: usize) {
        let held = std::mem::take(&mut self.panels.bots.live[at].held);
        if held.is_empty() {
            let id = self.panels.bots.live[at].bot.id.clone();
            self.publish_bot(&id, News::State("idle"));
        } else {
            self.bot_start(at, &held, Starting::chat());
        }
    }

    pub(in crate::app) fn bot_stop(&mut self, key: &str) -> Done<bool> {
        let at = self.bot_at(key)?;
        let state = &mut self.panels.bots.live[at];
        let held = std::mem::take(&mut state.held);
        let mut trees = Vec::new();
        for run in &mut state.runs {
            if !std::mem::replace(&mut run.stopping, true) {
                trees.push(run.running.clone());
            }
        }
        state.reports.clear();
        let chat_running = state.turn.is_some();
        let bot = state.bot.id.clone();
        if let Some(running) = state.turn.as_mut() {
            running.dropped.extend(held.iter().map(|(id, _)| *id));
            if !std::mem::replace(&mut running.stopping, true) {
                trees.push(running.running.clone());
            }
        }
        let any = chat_running || !self.panels.bots.live[at].runs.is_empty();
        for tree in trees {
            let job = Job::new(Level::Info, "bots", "stop").with("bot", &bot);
            self.spawn_job(job, move || Ok(tree.stop()), |_, _| None);
        }
        Ok(any)
    }

    pub(in crate::app) fn stop_bots(&mut self) {
        let trees: Vec<_> = self
            .panels
            .bots
            .live
            .iter_mut()
            .flat_map(|state| {
                let runs = state.runs.iter_mut().map(|run| {
                    run.stopping = true;
                    run.running.clone()
                });
                let turn = state.turn.as_mut().map(|turn| {
                    turn.stopping = true;
                    turn.running.clone()
                });
                runs.chain(turn).collect::<Vec<_>>()
            })
            .collect();
        let stopping: Vec<_> = trees.into_iter().map(|tree| std::thread::spawn(move || tree.stop())).collect();
        for handle in stopping {
            let _ = handle.join();
        }
    }

    pub(in crate::app) fn bot_reset(&mut self, key: &str) -> Done<()> {
        let at = self.bot_at(key)?;
        self.chat_idle(at, "is answering")?;
        let state = &mut self.panels.bots.live[at];
        state.bot.conversation = None;
        let _ = folder::save(&state.folder, &state.bot);
        self.bot_append(at, |id, now| Line::notice(id, now, chat::NEW_CONVERSATION));
        Ok(())
    }

    fn bot_idle(&self, at: usize, what: &str) -> Done<()> {
        self.busy_refusal(at, what, !self.panels.bots.live[at].runs.is_empty())
    }

    fn chat_idle(&self, at: usize, what: &str) -> Done<()> {
        self.busy_refusal(at, what, false)
    }

    fn busy_refusal(&self, at: usize, what: &str, extra: bool) -> Done<()> {
        let state = &self.panels.bots.live[at];
        if state.turn.is_some() || extra {
            let id = &state.bot.id;
            return Err(Refusal::new(Code::Busy, format!("{id} {what}; stop it first (cornercase bot stop {id})")));
        }
        Ok(())
    }

    pub(in crate::app) fn bot_remove(&mut self, key: &str) -> Done<(String, Vec<crate::queue::Entry>)> {
        let at = self.bot_at(key)?;
        self.bot_idle(at, "is answering")?;
        let state = &self.panels.bots.live[at];
        folder::remove(&state.folder).map_err(|error| Refusal::new(Code::Failed, error.to_string()))?;
        let removed = self.panels.bots.live.remove(at);
        self.forget_tags(&removed.bot.id);
        let entries = self.panels.queued.remove_bot(&removed.bot.id);
        if self.panels.bots.shown.as_ref().is_some_and(|shown| shown.bot == removed.bot.id) {
            self.leave_bot();
        }
        if self.overlay.as_ref().is_some_and(|overlay| overlay.names_bot(removed.key, &removed.bot.id)) {
            self.overlay = None;
        }
        self.clamp_bots_scroll();
        self.publish_bot_removed(&removed.bot.id);
        Ok((removed.bot.id, entries))
    }

    pub(in crate::app) fn bot_seen(&mut self, key: &str) -> Done<()> {
        let at = self.bot_at(key)?;
        self.panels.bots.live[at].unread = 0;
        Ok(())
    }

    pub(in crate::app) fn bot_answer(&self, at: usize, asked: u64) -> Option<Line> {
        self.panels.bots.live[at].answers.iter().find(|(id, _)| *id == asked).map(|(_, line)| line.clone())
    }
}
