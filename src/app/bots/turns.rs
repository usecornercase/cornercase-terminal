use std::time::{Duration, Instant};

use super::{BotEvent, News, Turn};
use crate::agents::Known;
use crate::agents::headless::{Ran, Watch};
use crate::agents::probe::Running;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::bots::chat::{self, Answer, Kind, Line};
use crate::bots::{folder, turn};
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
            .and_then(|bot| self.panels.bots.live.iter().position(|state| state.bot.id == bot.id))
            .ok_or_else(|| Refusal::new(Code::NotFound, format!("no bot {key}; cornercase bot list shows them")))
    }

    fn bot_append(&mut self, at: usize, make: impl FnOnce(u64, i64) -> Line) -> Line {
        let state = &mut self.panels.bots.live[at];
        let line = make(state.next_line, crate::time::now());
        state.next_line += 1;
        let _ = chat::append(&state.folder.join(folder::CHAT_FILE), &line);
        if matches!(line.kind, Kind::You | Kind::Bot) {
            state.last = Some(line.clone());
        }
        let id = state.bot.id.clone();
        self.publish_bot(&id, News::Line(line.clone()));
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
            self.bot_start(at, &[(line.id, text.to_string())]);
        }
        Ok((at, line))
    }

    fn bot_input(&self, at: usize, text: String) -> turn::Input {
        let state = &self.panels.bots.live[at];
        let kind = Known::of(&state.bot.agent);
        turn::Input {
            bot: state.bot.clone(),
            folder: state.folder.clone(),
            text,
            program: crate::agents::command(&self.config, &state.bot.agent),
            home: self.account_home(kind, state.bot.account.as_deref()),
            socket: self.shells.pane_socket().map(str::to_string),
            timeout: self.panels.bots.timeout,
        }
    }

    fn bot_start(&mut self, at: usize, asked: &[(u64, String)]) {
        let texts: Vec<String> = asked.iter().map(|(_, text)| text.clone()).collect();
        let input = self.bot_input(at, turn::joined(&texts));
        let (id, running) = (self.panels.bots.next_turn, Running::default());
        self.panels.bots.next_turn += 1;
        let bot = input.bot.id.clone();
        let job = Job::new(Level::Info, "bots", "turn").with("bot", &bot).with("agent", &input.bot.agent);
        let tx = self.tx.clone();
        let (steps_bot, ended_bot, started) = (bot.clone(), bot, Instant::now());
        let steps = Box::new(move |step| {
            let event = BotEvent::Step { bot: steps_bot.clone(), turn: id, step };
            let _ = tx.send(AppEvent::Bot(Box::new(event)));
        });
        let watch = Watch { steps: Some(steps), running: Some(running.clone()), ..Watch::default() };
        let state = &mut self.panels.bots.live[at];
        state.turn = Some(Turn {
            id,
            asked: asked.iter().map(|(line, _)| *line).collect(),
            running,
            stopping: false,
            dropped: Vec::new(),
        });
        self.spawn_job(
            job,
            move || Ok(turn::run(&input, watch)),
            move |result, _| {
                let (result, took) = (result.and_then(|inner| inner), started.elapsed());
                Some(AppEvent::Bot(Box::new(BotEvent::Ended { bot: ended_bot, turn: id, result, took })))
            },
        );
    }

    pub(in crate::app) fn bot_event(&mut self, event: BotEvent) {
        match event {
            BotEvent::Step { bot, turn, step } => self.bot_step(&bot, turn, &step),
            BotEvent::Ended { bot, turn, result, took } => self.bot_ended(&bot, turn, result, took),
        }
    }

    fn running_turn(&self, bot: &str, turn: u64) -> Option<usize> {
        let at = self.panels.bots.live.iter().position(|state| state.bot.id == bot)?;
        self.panels.bots.live[at].turn.as_ref().is_some_and(|running| running.id == turn).then_some(at)
    }

    fn bot_step(&mut self, bot: &str, turn: u64, step: &crate::agents::headless::Action) {
        if let Some(at) = self.running_turn(bot, turn) {
            self.bot_append(at, |id, now| Line::step(id, now, &step.tool, &step.text));
        }
    }

    fn bot_outcome(&mut self, at: usize, result: Result<Ran>, stopping: bool) -> (Line, &'static str) {
        let agent = self.panels.bots.live[at].bot.agent.clone();
        match result {
            Ok(ran) => {
                let state = &mut self.panels.bots.live[at];
                if ran.session.is_some() {
                    state.bot.conversation.clone_from(&ran.session);
                    let _ = folder::save(&state.folder, &state.bot);
                }
                let model = ran.model.clone().or_else(|| state.bot.model.clone());
                let answer = Answer { model, tokens: ran.tokens, conversation: ran.session };
                (self.bot_append(at, |id, now| Line::answer(id, now, &ran.text, answer)), "answered")
            }
            Err(_) if stopping => (self.bot_append(at, |id, now| Line::notice(id, now, chat::STOPPED)), "stopped"),
            Err(error) => {
                let text = turn::failure(&agent, &error, &self.panels.bots.live[at].bot.id);
                let how = if text.contains("did not answer in time") { "timeout" } else { "error" };
                (self.bot_append(at, |id, now| Line::failed(id, now, &text)), how)
            }
        }
    }

    fn bot_ended(&mut self, bot: &str, turn: u64, result: Result<Ran>, took: Duration) {
        let Some(at) = self.running_turn(bot, turn) else { return };
        let Some(finished) = self.panels.bots.live[at].turn.take() else { return };
        let tokens = result.as_ref().map_or(0, |ran| ran.tokens.total());
        let (line, how) = self.bot_outcome(at, result, finished.stopping);
        let dropped = self.bot_dropped(at, &finished);
        let state = &mut self.panels.bots.live[at];
        let shown = finished.asked.iter().any(|id| state.waited.contains(id));
        self.bot_answer_waiters(at, &finished.asked, &line);
        if let Some(notice) = dropped {
            self.bot_answer_waiters(at, &finished.dropped, &notice);
        }
        if !shown && matches!(line.kind, Kind::Bot | Kind::Error) {
            self.bot_notify(at, &line);
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

    fn bot_notify(&mut self, at: usize, line: &Line) {
        let state = &mut self.panels.bots.live[at];
        state.unread += 1;
        let first = line.text.lines().find(|text| !text.trim().is_empty()).unwrap_or_default();
        let text = crate::text::truncate_right(&crate::text::one_line(first), TOAST_WIDTH);
        let message = crate::host::notify::clean(&format!("{}: {text}", state.bot.name));
        let status = if line.kind == Kind::Error {
            crate::agents::activity::Status::Failed
        } else {
            crate::agents::activity::Status::Done
        };
        self.messages.toast =
            Some(super::super::toast::Toast::new(message.clone(), crate::ui::ToastIcon::Agent(status)));
        if let Some(note) = crate::host::notify::Notification::new(&message, &self.config.desktop_notifications) {
            self.outbox.notify(Some(note));
        }
    }

    fn bot_next(&mut self, at: usize) {
        let held = std::mem::take(&mut self.panels.bots.live[at].held);
        if held.is_empty() {
            let id = self.panels.bots.live[at].bot.id.clone();
            self.publish_bot(&id, News::State("idle"));
        } else {
            self.bot_start(at, &held);
        }
    }

    pub(in crate::app) fn bot_stop(&mut self, key: &str) -> Done<bool> {
        let at = self.bot_at(key)?;
        let state = &mut self.panels.bots.live[at];
        let held = std::mem::take(&mut state.held);
        let Some(running) = state.turn.as_mut() else { return Ok(false) };
        running.dropped.extend(held.iter().map(|(id, _)| *id));
        if std::mem::replace(&mut running.stopping, true) {
            return Ok(true);
        }
        let tree = running.running.clone();
        let job = Job::new(Level::Info, "bots", "stop").with("bot", &state.bot.id);
        self.spawn_job(job, move || Ok(tree.stop()), |_, _| None);
        Ok(true)
    }

    pub(in crate::app) fn stop_bots(&mut self) {
        let trees: Vec<_> = self
            .panels
            .bots
            .live
            .iter_mut()
            .filter_map(|state| {
                let turn = state.turn.as_mut()?;
                turn.stopping = true;
                Some(turn.running.clone())
            })
            .collect();
        let stopping: Vec<_> = trees.into_iter().map(|tree| std::thread::spawn(move || tree.stop())).collect();
        for handle in stopping {
            let _ = handle.join();
        }
    }

    pub(in crate::app) fn bot_reset(&mut self, key: &str) -> Done<()> {
        let at = self.bot_at(key)?;
        self.bot_idle(at, "is answering")?;
        let state = &mut self.panels.bots.live[at];
        state.bot.conversation = None;
        let _ = folder::save(&state.folder, &state.bot);
        self.bot_append(at, |id, now| Line::notice(id, now, chat::NEW_CONVERSATION));
        Ok(())
    }

    fn bot_idle(&self, at: usize, what: &str) -> Done<()> {
        let state = &self.panels.bots.live[at];
        if state.turn.is_some() {
            let id = &state.bot.id;
            return Err(Refusal::new(Code::Busy, format!("{id} {what}; stop it first (cornercase bot stop {id})")));
        }
        Ok(())
    }

    pub(in crate::app) fn bot_remove(&mut self, key: &str) -> Done<String> {
        let at = self.bot_at(key)?;
        self.bot_idle(at, "is answering")?;
        let state = &self.panels.bots.live[at];
        folder::remove(&state.folder).map_err(|error| Refusal::new(Code::Failed, error.to_string()))?;
        Ok(self.panels.bots.live.remove(at).bot.id)
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
