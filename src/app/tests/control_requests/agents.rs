use super::*;

fn known_conversation() -> (Agent, u64) {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.refreshing("the pane knows its conversation", |a| {
        pane(a, id).context.record_for(crate::agents::Known::Claude).is_some()
    });
    (agent, id)
}

fn finish_working(agent: &mut Agent, id: u64) -> Vec<Response> {
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    let early = answers(&mut agent.app);
    agent.finish();
    early
}

mod work_stats {
    use super::*;

    fn sample(agent: &mut Agent) {
        let now = Instant::now();
        agent.app.agents.set_watched(Some(now.checked_sub(Duration::from_secs(1)).expect("previous read")));
        agent.app.watch_agents(now);
    }
    fn total(agent: &Agent) -> crate::agents::stats::Window {
        agent.app.stats_view(SystemTime::now()).windows["total"].clone()
    }

    #[test]
    fn working_grows_time_and_finishing_counts_a_turn_and_the_recorded_tokens() {
        let mut agent = Agent::new();
        let id = agent.start(Some("synthetic work"));
        agent.refreshing("agent working", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
        let before = total(&agent);
        sample(&mut agent);
        let working = total(&agent);
        assert!(working.counts.agent_seconds >= before.counts.agent_seconds + 0.9);
        agent.finish();
        agent.refreshing("agent idle", |a| pane(a, id).agent.status() != Some(activity::Status::Working));
        let after = total(&agent);
        assert_eq!(
            (after.counts.agents, after.counts.prompts, after.counts.turns, after.counts.tokens.out),
            (1, 1, 1, 50)
        );
        assert_eq!(after.counts.tokens.input, 20);
        assert_eq!(after.counts.tokens.cache_write, 10);
        let value = value(&mut agent.app, None, Command::Stats(wire::Stats {}));
        assert_eq!(value["windows"]["total"]["tokens"]["cache_read"], 200);
    }

    #[test]
    fn background_shells_stop_agent_time() {
        let mut agent = Agent::new();
        let id = agent.start(Some("background"));
        agent.refreshing("agent working", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
        agent.finish();
        agent.refreshing("background shell", |a| pane(a, id).agent.background_shell());
        sample(&mut agent);
        let before = total(&agent);
        sample(&mut agent);
        assert_eq!(total(&agent).counts.agent_seconds, before.counts.agent_seconds);
        assert_eq!(total(&agent).counts.turns, 1);
        agent.finish();
    }

    #[test]
    fn waiting_and_done_in_an_unseen_tab_add_waited_time() {
        let mut agent = Agent::new();
        let id = agent.start(Some("please ask"));
        agent.finish();
        agent.refreshing("waiting", |a| pane(a, id).agent.status() == Some(activity::Status::Waiting));
        let before = total(&agent).counts.waited_seconds;
        sample(&mut agent);
        assert!(total(&agent).counts.waited_seconds >= before + 0.9);
        done(now(
            &mut agent.app,
            None,
            Command::Keys(wire::Keys { pane: Some(id), tab: None, keys: vec!["y".into(), "enter".into()] }),
        ));
        agent.refreshing("unseen done", |a| pane(a, id).agent.status() == Some(activity::Status::Done));
        sample(&mut agent);
        let before = total(&agent).counts.waited_seconds;
        sample(&mut agent);
        assert!(total(&agent).counts.waited_seconds >= before + 0.9);
    }
}

#[test]
fn start_types_the_agent_and_submits_its_prompt_in_a_tab_of_its_own() {
    let mut agent = Agent::new();
    let before = agent.app.focus();

    let id = agent.start(Some("fix the login"));

    agent.until("the agent reads the prompt", |a| pane_screen(a, id).contains("❯ fix the login"));
    let tab = &agent.app.projects[0].workspaces[0].tabs[1];
    assert_eq!((tab.name.as_deref(), agent.app.focus()), (Some("fixer"), before));
    agent.finish();
}

#[test]
fn send_and_wait_follows_the_agent_until_it_stops() {
    let mut agent = Agent::new();
    let id = agent.start(None);

    ask(&mut agent.app, None, send_text(id, "next step", true, true));
    let early = finish_working(&mut agent, id);

    let ended = done(agent.answered("the agent stops"));
    assert_eq!((early, ended.ended.as_deref()), (Vec::new(), Some("done")));
}

#[test]
fn a_wait_after_a_prompt_waits_for_the_agent_to_take_it() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    ask(&mut agent.app, None, send_text(id, "next step", true, false));
    done(agent.answered("the enter is pressed"));

    ask(&mut agent.app, None, wait_for(id, Until::Stops, None));
    let early = finish_working(&mut agent, id);

    let ended = done(agent.answered("the agent stops"));
    assert_eq!((early, ended.ended.is_some()), (Vec::new(), true));
}

#[test]
fn a_turn_over_with_a_background_shell_ends_only_a_wait_for_the_turn() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    let send = wire::SendText {
        pane: Some(id),
        text: Some("watch the tests in the background".into()),
        enter: true,
        wait: true,
        until: Until::TurnOver,
        ..wire::SendText::default()
    };
    ask(&mut agent.app, None, Command::Send(send));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    agent.finish();
    let turn = done(agent.answered("the turn is over"));

    let report: Report =
        serde_json::from_value(value(&mut agent.app, None, Command::Status(wire::Status {}))).expect("a report");
    let info = report
        .projects
        .iter()
        .flat_map(|p| &p.workspaces)
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .find(|p| p.id == id)
        .expect("the agent's pane");
    ask(&mut agent.app, None, wait_for(id, Until::Stops, Some(0.3)));
    let timed_out = error(agent.answered("the default wait times out"));
    agent.finish();
    ask(&mut agent.app, None, wait_for(id, Until::Stops, None));
    let ended = done(agent.answered("the background shell ends"));

    assert_eq!(
        (turn.ended.as_deref(), info.status.as_deref(), info.background_shell),
        (Some("shell"), Some("working"), true)
    );
    assert!(timed_out.contains("--until turn-over"), "{timed_out}");
    assert_eq!(ended.ended.as_deref(), Some("done"));
}

pub(super) fn in_status(app: &mut App, id: u64) -> wire::PaneInfo {
    let report: Report = serde_json::from_value(value(app, None, Command::Status(wire::Status {}))).expect("a report");
    let mut tabs = report.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
    tabs.find_map(|t| t.panes.iter().find(|p| p.id == id)).expect("the agent's pane").clone()
}

#[test]
fn send_refuses_an_agent_showing_a_panel_and_says_when_a_forced_prompt_was_not_taken() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.until("its input box is seen", |a| pane(a, id).input_seen.is_some());
    done(now(&mut agent.app, None, press_keys(id, &["p", "a", "n", "e", "l", "enter"])));
    agent.until("the panel shows", |a| pane_screen(a, id).contains("Shell details"));

    let shown = in_status(&mut agent.app, id).dialog;
    let refused = error(now(&mut agent.app, None, send_text(id, "next step", true, false)));
    agent.app.clients.set_confirm_within(Duration::from_millis(300));
    let forced = wire::SendText {
        pane: Some(id),
        text: Some("next step".into()),
        enter: true,
        force: true,
        ..wire::SendText::default()
    };
    ask(&mut agent.app, None, Command::Send(forced));
    let unconfirmed = error(agent.answered("the confirmation gives up"));
    agent.until("the panel closes", |a| pane_screen(a, id).contains('❯'));

    assert_eq!((shown, in_status(&mut agent.app, id).dialog), (true, false));
    assert!(refused.contains("shows a dialog, a panel or its shell mode"), "{refused}");
    assert!(unconfirmed.starts_with("not confirmed: the prompt may not have been submitted"), "{unconfirmed}");
}

#[test]
fn send_refuses_an_agent_showing_the_survey_until_keys_dismiss_it() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.until("its input box is seen", |a| pane(a, id).input_seen.is_some());
    done(now(&mut agent.app, None, press_keys(id, &["s", "u", "r", "v", "e", "y", "enter"])));
    agent.until("the survey shows", |a| pane_screen(a, id).contains("0: Dismiss"));

    let shown = in_status(&mut agent.app, id);
    let refused = error(now(&mut agent.app, None, send_text(id, "2", true, false)));
    done(now(&mut agent.app, None, press_keys(id, &["0", "enter"])));
    agent.until("the survey goes", |a| !pane_screen(a, id).contains("0: Dismiss"));
    let gone = in_status(&mut agent.app, id);
    ask(&mut agent.app, None, send_text(id, "2", true, false));
    done(agent.answered("the agent records the prompt"));

    assert_eq!([(shown.survey, shown.dialog), (gone.survey, gone.dialog)], [(true, false), (false, false)]);
    assert!(refused.contains(&format!("dismiss it with `cornercase keys --pane {id} 0`")), "{refused}");
}

#[test]
fn a_wait_on_several_ends_at_the_turn_over_of_an_agent_with_a_background_shell() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    let tab = agent.app.projects[0].workspaces[0].tabs[1].id;
    ask(&mut agent.app, None, send_text(id, "watch the tests in the background", true, false));
    done(agent.answered("the enter is pressed"));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    agent.finish();
    agent.refreshing("the turn is over", |a| pane(a, id).agent.background_shell());

    ask(&mut agent.app, None, wait_on(&[id], &[tab], false, Until::TurnOver, None));
    let any = done(agent.answered("the first one's turn is over"));
    ask(&mut agent.app, None, wait_on(&[id], &[tab], true, Until::TurnOver, None));
    let all = done(agent.answered("every turn is over"));
    ask(&mut agent.app, None, wait_on(&[id], &[], true, Until::Stops, Some(0.3)));
    let timed_out = error(agent.answered("waiting for it to stop times out"));
    agent.finish();

    let endings = |done: &Done| done.panes.iter().map(|p| (p.ids.tab, p.ids.pane, p.ended.clone())).collect::<Vec<_>>();
    let shell = Some("shell".to_string());
    assert_eq!(endings(&any), [(None, Some(id), shell.clone())]);
    assert_eq!(endings(&all), [(None, Some(id), shell.clone()), (Some(tab), Some(id), shell)]);
    assert!(timed_out.contains("--until turn-over"), "{timed_out}");
}

#[test]
fn events_tell_each_change_of_what_the_agent_does() {
    let mut agent = Agent::new();
    follow(&mut agent.app, &[]);
    let id = agent.start(None);
    let says = |expected: Vec<(Option<String>, Option<String>)>| {
        move |streamed: &[Streamed]| statuses(streamed, id) == expected
    };
    let (app, rx) = (&mut agent.app, &agent.rx);
    let mut got = until_told(app, rx, "the agent shows up", says(vec![change(None, Some("idle"))]));

    ask(&mut agent.app, None, send_text(id, "next step", true, false));
    done(agent.answered("the enter is pressed"));
    let (app, rx) = (&mut agent.app, &agent.rx);
    got.extend(until_told(app, rx, "the agent works", says(vec![change(Some("idle"), Some("working"))])));
    agent.finish();
    let (app, rx) = (&mut agent.app, &agent.rx);
    got.extend(until_told(app, rx, "the agent is done", says(vec![change(Some("working"), Some("done"))])));

    let agents: Vec<String> = told(&got)
        .into_iter()
        .filter_map(|(what, _)| match what {
            wire::What::Status { agent, .. } => Some(agent),
            _ => None,
        })
        .collect();
    assert_eq!(agents, ["claude"; 3]);
}

#[test]
fn send_refuses_an_agent_waiting_for_an_answer() {
    let mut agent = Agent::new();
    let id = agent.start(Some("please ask"));
    agent.finish();
    agent.refreshing("the agent asks", |a| pane(a, id).agent.status() == Some(activity::Status::Waiting));

    let message = error(now(&mut agent.app, None, send_text(id, "yes", true, false)));

    assert!(message.contains("waiting for an answer"), "{message}");
    assert_eq!(agent.status(id), Some(activity::Status::Waiting));
}

fn last_message(pane: u64) -> Command {
    Command::LastMessage(wire::LastMessage { pane: Some(pane), tab: None })
}

#[test]
fn read_last_message_gives_what_the_agent_wrote_last_and_whether_its_turn_is_over() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.transcript(id, &[
        r#"{"type":"user","message":{"role":"user","content":"fix the login"}}"#,
        r#"{"type":"assistant","timestamp":"2026-10-08T19:56:37.241Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"Fixed the login form."}]}}"#,
    ]);
    ask(&mut agent.app, None, send_text(id, "next step", true, false));
    done(agent.answered("the enter is pressed"));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));

    ask(&mut agent.app, None, last_message(id));
    let working = done(agent.answered("the transcript is read"));
    agent.finish();
    agent.refreshing("the agent stops", |a| pane(a, id).agent.status() != Some(activity::Status::Working));
    ask(&mut agent.app, None, last_message(id));
    let over = done(agent.answered("the transcript is read again"));

    assert_eq!(working.text.as_deref(), Some("Fixed the login form."));
    assert_eq!(
        (working.agent.as_deref(), working.written.as_deref()),
        (Some("claude"), Some("2026-10-08T19:56:37.241Z"))
    );
    assert_eq!((working.turn_over, over.turn_over), (Some(false), Some(true)));
    assert_eq!(working.ids.pane, Some(id));
}

#[test]
fn the_record_of_another_agent_is_not_read() {
    let (mut agent, id) = known_conversation();
    let term = agent.app.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id);
    term.expect("the agent's pane").agent.follow(Some(crate::agents::Known::Codex.name()));

    let message = error(now(&mut agent.app, None, last_message(id)));

    assert!(message.starts_with(&format!("cornercase has not found where the codex agent in pane {id}")), "{message}");
}

#[test]
fn an_agent_that_has_not_written_yet_has_no_last_message() {
    let (mut agent, id) = known_conversation();

    ask(&mut agent.app, None, last_message(id));

    let message = error(agent.answered("the transcript is looked for"));
    assert_eq!(message, format!("the claude agent in pane {id} has not written a message yet"));
}

mod restarting_when_idle {
    use super::*;

    fn finish_and_recheck(agent: &mut Agent, id: u64) {
        agent.finish();
        agent.refreshing("the agent stops", |a| pane(a, id).agent.status() != Some(activity::Status::Working));
        looked_again(agent);
    }

    fn when_idle(timeout: Option<f64>) -> Command {
        Command::RestartWhenIdle(wire::RestartWhenIdle { timeout })
    }

    pub(super) fn working(agent: &mut Agent, prompt: &str) -> u64 {
        let id = agent.start(None);
        ask(&mut agent.app, None, send_text(id, prompt, true, false));
        done(agent.answered("the enter is pressed"));
        agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
        id
    }

    fn looked_again(agent: &mut Agent) {
        let since = Instant::now();
        agent.refreshing("another look at the agents", |a| a.agents.watched().is_some_and(|at| at > since));
    }

    fn report(response: Response) -> Report {
        match response {
            Response::Ok(value) => serde_json::from_value(value).expect("a report"),
            Response::Error(message) => panic!("the restart failed: {message}"),
        }
    }

    #[test]
    fn waits_until_the_agent_ends_its_turn_then_restarts() {
        let mut agent = Agent::new();
        let id = working(&mut agent, "next step");

        ask(&mut agent.app, None, when_idle(None));
        looked_again(&mut agent);
        let pending = (answers(&mut agent.app), toast(&agent.app).map(str::to_owned));
        let early = agent.app.take_restart();
        agent.finish();
        let stopped = report(agent.answered("the agent ends its turn"));

        assert_eq!(pending, (Vec::new(), Some("restart pending until 1 agent ends its turn".into())));
        let notice = agent.app.messages.toast.as_ref().map(|t| t.icon);
        assert_eq!((early, agent.app.take_restart().is_some()), (None, true));
        assert_ne!(notice, Some(ui::ToastIcon::Restart));
        let panes = stopped.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
        let info = panes.flat_map(|t| &t.panes).find(|p| p.id == id).expect("the agent's pane");
        assert_eq!(info.status.as_deref(), Some("done"));
    }

    #[test]
    fn a_turn_left_with_a_background_shell_is_over() {
        let mut agent = Agent::new();
        let id = working(&mut agent, "watch in the background");
        agent.finish();
        agent.refreshing("the turn is over", |a| pane(a, id).agent.background_shell());

        ask(&mut agent.app, None, when_idle(None));
        report(agent.answered("the restart"));

        assert!(agent.app.take_restart().is_some());
        agent.finish();
    }

    #[test]
    fn the_pane_that_asks_never_holds_it() {
        let mut agent = Agent::new();
        let id = working(&mut agent, "update cornercase");

        ask(&mut agent.app, Some(id), when_idle(None));
        report(agent.answered("the restart"));

        assert!(agent.app.take_restart().is_some());
        agent.finish();
    }

    #[test]
    fn what_was_just_typed_into_an_agent_holds_it_a_moment() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        done(now(&mut agent.app, None, send_text(id, "half a thought", false, false)));
        ask(&mut agent.app, None, when_idle(None));
        let typed = Instant::now();

        agent.app.agents.set_watched(None);
        agent.app.refresh(typed);
        let held = (answers(&mut agent.app), agent.app.take_restart());
        agent.app.agents.set_watched(None);
        agent.app.refresh(typed + Duration::from_secs(3));

        assert_eq!(held, (Vec::new(), None));
        assert!(agent.app.take_restart().is_some());
    }

    #[test]
    fn a_timeout_gives_up_and_keeps_the_server() {
        let mut agent = Agent::new();
        let id = working(&mut agent, "next step");

        ask(&mut agent.app, None, when_idle(Some(0.05)));
        let message = error(agent.answered("the timeout"));

        assert_eq!(
            message,
            format!(
                "timed out after 0.05s: the agent in pane {id} has not ended its turn, so the server \
                             keeps running"
            )
        );
        assert_eq!((agent.app.take_restart(), toast(&agent.app)), (None, None));
        agent.finish();
    }

    #[test]
    fn the_window_cancels_it() {
        let mut agent = Agent::new();
        let id = working(&mut agent, "next step");
        ask(&mut agent.app, None, when_idle(None));

        let shown = agent.app.messages.toast.as_ref().expect("the pending restart").view();
        let cancel = ui::toast_button(AREA, shown).as_position();
        click(&mut agent.app, cancel);
        let message = error(answers(&mut agent.app).remove(0));
        finish_and_recheck(&mut agent, id);

        assert_eq!(message, "the restart was cancelled in the window; the server keeps running");
        assert_eq!((agent.app.take_restart(), toast(&agent.app)), (None, None));
    }

    #[test]
    fn a_command_that_goes_away_takes_it_along() {
        let mut agent = Agent::new();
        let id = working(&mut agent, "next step");
        ask(&mut agent.app, None, when_idle(None));

        agent.app.forget(CLIENT);
        finish_and_recheck(&mut agent, id);

        let answered = answers(&mut agent.app);
        assert_eq!((agent.app.take_restart(), toast(&agent.app), answered), (None, None, Vec::new()));
    }
}

mod keeping_awake {
    use super::restarting_when_idle::working;
    use super::*;
    use crate::agents::awake::{KEEP_FOR, State, Tool};
    use crate::control::KeepAwake;

    fn holding(agent: &mut Agent) -> TempDir {
        let fakes = TempDir::new();
        let script = fakes.path().join("holds");
        write_executable(&script, "#!/bin/sh\nexec cat\n");
        agent.app.agents.awake = Awake::new(vec![Tool::new("holds", script, &[])]);
        agent.app.config.keep_awake = true;
        fakes
    }

    fn later(agent: &mut Agent, by: Duration) -> State {
        agent.app.agents.set_watched(None);
        agent.app.refresh(Instant::now() + by);
        agent.app.agents.awake.state()
    }

    #[test]
    fn holds_while_an_agent_works_and_lets_go_after_the_grace() {
        let mut agent = Agent::new();
        let _fakes = holding(&mut agent);
        let id = working(&mut agent, "next step");
        let held = (agent.app.agents.awake.state(), agent.app.report(None).keep_awake);
        assert_eq!(agent.app.app_state().keep_awake, held.1);
        agent.finish();
        agent.refreshing("the turn ends", |a| pane(a, id).agent.status() != Some(activity::Status::Working));

        let kept = later(&mut agent, Duration::ZERO);
        let released = later(&mut agent, KEEP_FOR + Duration::from_secs(1));

        let report = KeepAwake { held: true, by: Some("holds".into()), unavailable: None };
        assert_eq!(held, (State::Held("holds".into()), Some(report)));
        assert_eq!((kept, released), (State::Held("holds".into()), State::Released));
    }

    #[test]
    fn a_turn_left_with_a_background_shell_does_not_hold_it() {
        let mut agent = Agent::new();
        let _fakes = holding(&mut agent);
        let id = working(&mut agent, "watch in the background");
        agent.finish();
        agent.refreshing("the turn is over", |a| pane(a, id).agent.background_shell());

        later(&mut agent, Duration::ZERO);
        let released = later(&mut agent, KEEP_FOR + Duration::from_secs(1));

        assert_eq!(released, State::Released);
        agent.finish();
    }

    #[test]
    fn is_left_out_of_the_report_while_the_setting_is_off() {
        let agent = Agent::new();

        assert_eq!(agent.app.report(None).keep_awake, None);
    }
}

mod pending_review {
    use super::*;

    fn mark(pane: u64, reviewed: bool) -> Command {
        Command::Mark(wire::Mark { pane: Some(pane), tab: None, reviewed })
    }

    fn reported(agent: &mut Agent, id: u64) -> Option<String> {
        let value = value(&mut agent.app, None, Command::Status(wire::Status::default()));
        let report: Report = serde_json::from_value(value).expect("a report");
        report.panes().into_iter().find(|row| row.pane == id).and_then(|row| row.status)
    }

    fn finished() -> (Agent, u64) {
        let mut agent = Agent::new();
        let id = agent.start(Some("fix the login"));
        finish_working(&mut agent, id);
        agent.refreshing("the turn is over and the conversation known", |a| {
            pane(a, id).agent.status().is_some_and(|s| s != activity::Status::Working) && pane(a, id).resume.is_some()
        });
        (agent, id)
    }

    #[test]
    fn a_marked_pane_reports_pending_until_it_is_reviewed() {
        let (mut agent, id) = finished();

        assert_eq!(done(now(&mut agent.app, None, mark(id, false))).ids.pane, Some(id));
        let marked = reported(&mut agent, id);
        agent.refreshing("a refresh in sight", |_| true);
        let after_a_look = reported(&mut agent, id);
        done(now(&mut agent.app, None, mark(id, true)));

        assert_eq!(
            (marked.as_deref(), after_a_look.as_deref(), reported(&mut agent, id).as_deref()),
            (Some("pending"), Some("pending"), Some("idle"))
        );
    }

    #[test]
    fn a_pane_without_an_agent_cannot_be_marked() {
        let mut agent = Agent::new();
        agent.start(None);
        let shell = done(now(&mut agent.app, None, new_tab(None))).ids.pane.expect("the shell's pane");

        let message = error(now(&mut agent.app, None, mark(shell, false)));

        assert!(message.contains("nothing to mark"), "{message}");
    }

    #[test]
    fn a_pending_mark_comes_back_with_the_conversation_after_a_restart() {
        let (mut agent, id) = finished();
        done(now(&mut agent.app, None, mark(id, false)));
        let saved = agent.app.state();
        let panes = saved.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs).flat_map(|t| &t.panes);
        let marked: Vec<_> = panes.map(|p| (p.agent.is_some(), p.pending)).filter(|&(agent, _)| agent).collect();
        assert_eq!(marked, [(true, true)]);

        let (mut back, rx) = new_app(agent.config_dir.path().join("config.json"));
        back.agents.set_claude_dir(agent.fake.dir());
        assert!(back.restore(&saved, AREA));
        refreshing(&mut back, &rx, "the resumed agent shows its mark", |a| {
            a.projects
                .iter()
                .flat_map(|p| &p.workspaces)
                .flat_map(Workspace::terms)
                .any(|t| t.agent.agent().is_some() && t.agent.status() == Some(activity::Status::Pending))
        });
    }
}
