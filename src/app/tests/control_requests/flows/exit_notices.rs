use super::exited::{keep_conversation, kill_role, team_reopens};
use super::*;
use crate::agents::parent::Parent;
use crate::app::WATCH_AGENTS_EVERY;

const NOTICES: u64 = CLIENT;
const OTHERS: u64 = CLIENT + 1;

fn listening(team: &mut Team) {
    team.app.clients.events.subscribe(NOTICES, Vec::new(), false, true);
    team.app.clients.events.subscribe(OTHERS, Vec::new(), false, false);
}

fn reads(team: &mut Team, count: u32) {
    let start = team.app.agents.watched().unwrap_or_else(Instant::now);
    let until = start + WATCH_AGENTS_EVERY * count;
    refreshing(&mut team.app, &team.rx, "the agents are read again", |app| {
        app.agents.watched().is_some_and(|at| at >= until)
    });
}

fn exits(team: &mut Team, heard: &mut Vec<(u64, Streamed)>, reader: u64) -> Vec<Exit> {
    heard.extend(team.app.take_events());
    flow_exits(heard, reader)
}

fn until_exited(team: &mut Team, pane: u64) {
    refreshing(&mut team.app, &team.rx, "the role exited", |app| app.runs.all()[0].exited == [pane]);
    reads(team, 3);
}

fn ready() -> (Team, u64) {
    let mut team = Team::new();
    team.started();
    team.until_got(3);
    let reviewer = team.app.runs.all()[0].pane_of("reviewer").expect("the reviewer");
    (team, reviewer)
}

fn assert_toasted(team: &mut Team) {
    let toast = toast(&team.app).expect("a toast").to_string();
    assert!(toast.contains("reviewer's agent exited in"), "{toast}");
    assert!(toast.ends_with(" · ▶ on the workspace reopens it"), "{toast}");
    assert_eq!(team.app.take_notifications().len(), 1);
}

fn reopen_reviewer(team: &mut Team) {
    keep_conversation(team, "reviewer", "c2");
    team_reopens(team);
    let log = team.log.path().to_path_buf();
    until_logged(&mut team.app, &team.rx, "the reviewer starts again", || {
        read_order(&log).iter().any(|line| line.contains("--resume c2"))
    });
    reads(team, 2);
}

fn settled_again(team: &mut Team, pane: u64) {
    team.app.agents.launches.drop_panes(&[pane]);
    team.app.projects.pane_by_mut(pane).expect("the pane").resuming = None;
}

#[test]
fn a_roles_exit_is_told_once_and_again_after_it_comes_back_and_exits() {
    let (mut team, reviewer) = ready();
    listening(&mut team);
    let run = team.app.runs.all()[0].id;
    let tab = team.app.projects.tab_of_pane(reviewer).map(|at| team.app.projects.tab(at).id).expect("its tab");
    assert!(!team.app.seen_tabs().contains(&tab));
    let _ = team.app.take_notifications();
    let mut heard = Vec::new();

    kill_role(&team, "reviewer");
    until_exited(&mut team, reviewer);

    let told = vec![(vec![reviewer], Some(run), Some("reviewer".to_string()))];
    assert_eq!(exits(&mut team, &mut heard, NOTICES), told);
    assert_eq!(exits(&mut team, &mut heard, OTHERS), Vec::<Exit>::new());
    assert_toasted(&mut team);

    let workspace = team.app.runs.all()[0].workspace;
    ok(&mut team.app, "flow-runs", &json!({}));
    ok(&mut team.app, "status", &json!({}));
    reads(&mut team, 2);
    assert_eq!(exits(&mut team, &mut heard, NOTICES), told, "nothing more was told");

    reopen_reviewer(&mut team);
    assert_eq!(exits(&mut team, &mut heard, NOTICES), told, "nothing while it comes back");

    settled_again(&mut team, reviewer);
    kill_role(&team, "reviewer");
    until_exited(&mut team, reviewer);
    assert_eq!(exits(&mut team, &mut heard, NOTICES).len(), 2, "a second exit is told again");
    assert_eq!(team.app.runs.all()[0].workspace, workspace);
}

#[test]
fn an_exit_told_before_a_restart_is_not_told_again() {
    let (mut team, reviewer) = ready();
    kill_role(&team, "reviewer");
    until_exited(&mut team, reviewer);
    let saved = team.app.state();
    let run = saved.projects[0].workspaces.iter().find_map(|w| w.run.as_ref()).expect("the run is saved");
    assert_eq!(run.exits_told, ["reviewer"]);
    let (mut again, again_rx) = new_app(team.config.path().join("config.json"));
    again.clients.events.subscribe(NOTICES, Vec::new(), false, true);

    assert!(again.restore(&saved, AREA));
    let pane = again.runs.all()[0].pane_of("reviewer").expect("the reviewer");
    refreshing(&mut again, &again_rx, "the reviewer is exited", |app| app.runs.all()[0].exited.contains(&pane));
    refreshing(&mut again, &again_rx, "another read", |app| app.agents.watched().is_some());

    let told: Vec<Exit> = flow_exits(&again.take_events(), NOTICES);
    assert!(told.iter().all(|(panes, _, _)| panes != &vec![pane]), "{told:?}");
}

#[test]
fn closing_a_runs_agents_tells_no_exit() {
    let (mut team, _) = ready();
    listening(&mut team);
    let workspace = team.app.runs.all()[0].workspace;
    let _ = team.app.take_events();

    ok(&mut team.app, "agents-close", &json!({"workspace": workspace, "force": true}));
    refreshing(&mut team.app, &team.rx, "the agents are closed", |app| app.runs.all().is_empty());
    reads(&mut team, 2);

    assert_eq!(flow_exits(&team.app.take_events(), NOTICES), Vec::<Exit>::new());
    assert!(!toast(&team.app).is_some_and(|toast| toast.contains("exited")));
}

#[test]
fn an_exit_in_the_tab_on_screen_gets_its_toast_but_no_notice() {
    let (mut team, reviewer) = ready();
    listening(&mut team);
    done(now(&mut team.app, None, Command::Focus(wire::Focus { item: Item::Pane(reviewer), reveal: None })));
    let _ = team.app.take_events();
    let _ = team.app.take_notifications();

    kill_role(&team, "reviewer");
    until_exited(&mut team, reviewer);

    assert!(toast(&team.app).is_some_and(|toast| toast.contains("reviewer's agent exited")));
    assert_eq!(team.app.take_notifications().len(), 0);
    assert_eq!(flow_exits(&team.app.take_events(), NOTICES), Vec::<Exit>::new());
}

#[test]
fn a_members_exit_is_silent_with_subagent_notices_off() {
    let mut agent = Agent::new();
    let [o, c] = wake::team(&mut agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    agent.app.clients.events.subscribe(NOTICES, Vec::new(), false, true);
    agent.app.projects.pane_by_mut(c).expect("the coder").parent = Some(Parent::pane(o));
    assert!(agent.app.subagent_panes().contains(&c));
    agent.app.config.notify_subagents = false;
    let _ = agent.app.take_events();

    let (pid, _) = pane(&agent.app, c).context.agent().expect("the agent's process");
    std::process::Command::new("kill").arg(pid.to_string()).status().expect("kill");
    agent.refreshing("the coder exited", |app| app.runs.all()[0].exited == [c]);
    wake::reads(&mut agent, 2);

    assert_eq!(flow_exits(&agent.app.take_events(), NOTICES), Vec::<Exit>::new());
    assert!(!toast(&agent.app).is_some_and(|toast| toast.contains("exited")));
    assert_eq!(agent.app.runs.all()[0].exits_told, ["coder"], "the exit was consumed");

    let (pid, _) = pane(&agent.app, o).context.agent().expect("the agent's process");
    std::process::Command::new("kill").arg(pid.to_string()).status().expect("kill");
    agent.refreshing("the orchestrator is told", |app| app.runs.all()[0].exits_told.len() == 2);
    assert!(toast(&agent.app).is_some_and(|toast| toast.contains("orchestrator's agent exited")));
}
