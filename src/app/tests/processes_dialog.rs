use std::time::Duration;

use super::close_agent::agent;
use super::*;
use crate::app::processes_dialog::ProcessesEvent;
use crate::process;
use crate::processes::dialog::{Action, Dialog, Hit, Line};
use crate::processes::{Pane, PaneTree, Proc, Sample, Times};
use crate::project::Hierarchy;

fn dialog(app: &mut App) -> &mut Dialog {
    match &mut app.overlay {
        Some(Overlay::Processes(dialog)) => dialog,
        _ => panic!("the processes dialog is not open"),
    }
}

fn opened(app: &mut App) -> u64 {
    app.open_processes();
    dialog(app).generation
}

fn pane_ids(app: &App) -> Vec<u64> {
    app.projects.terms().map(|term| term.id).collect()
}

fn proc(pid: i32, depth: usize, name: &str, cpu: f64) -> Proc {
    Proc {
        pid,
        parent: None,
        depth,
        name: name.into(),
        args: vec![name.into()],
        started: Some(1),
        cpu: Some(cpu),
        memory: Some(1 << 20),
    }
}

fn sample_of(cpus: &[(u64, f64)]) -> Sample {
    let panes = cpus
        .iter()
        .map(|(id, cpu)| {
            let shell = i32::try_from(1000 + id).expect("a pid");
            PaneTree {
                pane: Pane { id: *id, shell, foreground: None, agent: None },
                procs: vec![proc(shell, 0, "sh", 0.0), proc(shell + 1, 1, "node", *cpu)],
            }
        })
        .collect();
    Sample { panes, cornercase: Vec::new(), times: Times::default() }
}

fn sampled(app: &mut App, generation: u64, cpus: &[(u64, f64)]) {
    let result = Ok(sample_of(cpus));
    app.processes_answered(ProcessesEvent::Sampled { generation, result }, AREA);
}

fn order(app: &mut App) -> Vec<Option<u64>> {
    dialog(app).rows.iter().map(|row| row.pane).collect()
}

fn pointer_over_the_list(app: &mut App) {
    let list = ui::processes_room(AREA, false).1;
    app.screen.hover = Some(Position::new(list.x + 1, list.y));
}

fn toast(app: &App) -> String {
    app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default()
}

mod opening {
    use super::*;

    #[test]
    fn more_lists_processes_after_usage_and_opens_the_dialog() {
        let (mut app, _rx) = app();
        render(&mut app, AREA);
        let more = app_areas(&app).more.as_position();
        click(&mut app, more);
        assert_eq!(menu_labels(&app), ["settings", "usage", "processes", "quit"]);
        pick(&mut app, "processes");
        assert!(matches!(app.overlay, Some(Overlay::Processes(_))));
    }

    #[test]
    fn k_in_the_keys_menu_opens_it() {
        let (mut app, _rx) = app();
        app.config.prefix_key = "ctrl+]".into();
        send_key(&mut app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        send_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert!(matches!(app.overlay, Some(Overlay::Processes(_))));
    }

    #[test]
    fn esc_closes_it_and_a_click_outside_does_too() {
        let (mut app, _rx) = app();
        opened(&mut app);
        send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.overlay.is_none());
        opened(&mut app);
        click(&mut app, Position::new(0, 0));
        assert!(app.overlay.is_none());
    }
}

mod sampling {
    use super::*;

    #[test]
    fn is_asked_on_opening_and_every_two_seconds_while_open_and_never_twice_at_once() {
        let (mut app, rx, _dirs) = app_with(1);
        opened(&mut app);
        assert!(app.processes_sampling());
        wait_until("the first sample answers", || {
            drain(&mut app, &rx);
            dialog(&mut app).loaded
        });
        let now = Instant::now();
        app.watch_processes_dialog(now);
        assert!(!app.processes_sampling());
        app.watch_processes_dialog(now + Duration::from_secs(2));
        assert!(app.processes_sampling());
        app.watch_processes_dialog(now + Duration::from_secs(4));
        wait_until("the second sample answers", || {
            drain(&mut app, &rx);
            !app.processes_sampling()
        });
        app.overlay = None;
        app.watch_processes_dialog(now + Duration::from_secs(10));
        assert!(!app.processes_sampling());
    }

    #[test]
    fn an_answer_for_an_earlier_opening_is_dropped() {
        let (mut app, _rx, _dirs) = app_with(1);
        let earlier = opened(&mut app);
        let current = opened(&mut app);
        let pane = pane_ids(&app)[0];
        sampled(&mut app, earlier, &[(pane, 5.0)]);
        assert!(!dialog(&mut app).loaded);
        sampled(&mut app, current, &[(pane, 5.0)]);
        assert_eq!(order(&mut app), [Some(pane)]);
    }

    #[test]
    fn a_pane_that_closed_since_is_not_listed() {
        let (mut app, _rx, _dirs) = app_with(1);
        let generation = opened(&mut app);
        let pane = pane_ids(&app)[0];
        sampled(&mut app, generation, &[(pane, 5.0), (9999, 50.0)]);
        assert_eq!(dialog(&mut app).rows.len(), 1);
    }

    #[test]
    fn the_order_holds_while_the_pointer_is_over_the_list_and_sorts_again_once_it_leaves() {
        let (mut app, _rx, _dirs) = app_with(2);
        let generation = opened(&mut app);
        let ids = pane_ids(&app);
        let (a, b) = (ids[0], ids[1]);
        sampled(&mut app, generation, &[(a, 10.0), (b, 90.0)]);
        assert_eq!(order(&mut app), [Some(b), Some(a)]);
        pointer_over_the_list(&mut app);
        sampled(&mut app, generation, &[(a, 99.0), (b, 1.0)]);
        assert_eq!(order(&mut app), [Some(b), Some(a)]);
        assert_eq!(dialog(&mut app).rows[1].cpu, 99.0);
        app.screen.hover = None;
        sampled(&mut app, generation, &[(a, 99.0), (b, 1.0)]);
        assert_eq!(order(&mut app), [Some(a), Some(b)]);
    }
}

mod stopping {
    use super::*;

    fn shell_of(app: &App) -> i32 {
        app.projects.terms().next().and_then(Term::shell_pid).expect("a shell")
    }

    fn line_of(app: &mut App, pid: i32) -> Option<usize> {
        let dialog = dialog(app);
        let lines = dialog.visible();
        lines.iter().position(|line| match line {
            Line::Proc(at, proc) => dialog.row(*at).is_some_and(|row| row.procs[*proc].pid == pid),
            Line::Pane(_) => false,
        })
    }

    #[test]
    fn a_program_of_a_pane_is_listed_under_its_shell_and_stops_only_after_the_confirmation() {
        let (mut app, rx, _dirs) = app_with(1);
        type_line(&mut app, "sleep 30");
        let shell = shell_of(&app);
        wait_until("the sleep starts", || !process::children(shell).is_empty());
        let sleeper = process::children(shell)[0];
        opened(&mut app);
        let mut later = Instant::now();
        wait_until("the sleep is listed", || {
            later += Duration::from_secs(2);
            app.watch_processes_dialog(later);
            drain(&mut app, &rx);
            dialog(&mut app).rows.iter().any(|row| row.procs.iter().any(|proc| proc.pid == sleeper))
        });
        let action = dialog(&mut app).click(Hit::Row(0));
        app.processes_act(action);
        let at = line_of(&mut app, sleeper).expect("the sleep line");
        let action = dialog(&mut app).click(Hit::StopRow(at));
        app.processes_act(action);
        assert!(process::running(sleeper, None));
        let Some(Overlay::Processes(open)) = &app.overlay else { panic!("the dialog is open") };
        let confirm = app.processes_view(open, AREA).confirm.expect("a confirmation");
        let said: String = confirm.lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        assert!(said.contains(&format!("pid {sleeper}")), "{said}");
        let action = dialog(&mut app).click(Hit::Stop);
        app.processes_act(action);
        wait_until("the stop answers", || {
            drain(&mut app, &rx);
            !process::running(sleeper, None) && app.messages.toast.is_some()
        });
        assert_eq!(toast(&app), format!("stopped sleep (pid {sleeper})"));
        assert!(process::running(shell, None));
    }

    #[test]
    fn the_shell_line_offers_no_stop() {
        let (mut app, _rx, _dirs) = app_with(1);
        let generation = opened(&mut app);
        let pane = pane_ids(&app)[0];
        sampled(&mut app, generation, &[(pane, 5.0)]);
        let action = dialog(&mut app).click(Hit::Row(0));
        app.processes_act(action);
        let action = dialog(&mut app).click(Hit::StopRow(1));
        app.processes_act(action);
        let Some(Overlay::Processes(open)) = &app.overlay else { panic!("the dialog is open") };
        assert!(app.processes_view(open, AREA).confirm.is_none());
    }
}

mod go_and_close {
    use super::*;

    #[test]
    fn go_closes_the_dialog_and_shows_the_pane_even_in_another_project() {
        let (mut app, _rx, _dirs) = app_with(2);
        let generation = opened(&mut app);
        let ids = pane_ids(&app);
        sampled(&mut app, generation, &[(ids[0], 1.0), (ids[1], 50.0)]);
        let at = dialog(&mut app).rows.iter().position(|row| row.pane == Some(ids[0])).expect("a row");
        let action = dialog(&mut app).click(Hit::Go(at));
        app.processes_act(action);
        assert!(app.overlay.is_none());
        assert_eq!(app.term().map(|term| term.id), Some(ids[0]));
    }

    #[test]
    fn close_on_a_tabs_last_pane_asks_to_close_the_tab() {
        let (mut app, _rx, _dirs) = app_with(1);
        opened(&mut app);
        let pane = pane_ids(&app)[0];
        app.processes_act(Action::ClosePane(pane));
        assert!(matches!(app.overlay, Some(Overlay::CloseTab { .. })), "{:?}", app.overlay.as_ref().map(Overlay::name));
    }

    #[test]
    fn close_in_a_split_tab_asks_to_close_the_pane() {
        let (mut app, _rx, _dirs) = app_with(1);
        app.config.prefix_key = "ctrl+]".into();
        send_key(&mut app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        send_key(&mut app, KeyCode::Char('|'), KeyModifiers::NONE);
        let pane = app.tab().map(|tab| tab.panes[1].id).expect("a second pane");
        opened(&mut app);
        app.processes_act(Action::ClosePane(pane));
        assert!(matches!(app.overlay, Some(Overlay::ClosePane { pane: asked }) if asked == pane));
    }

    #[test]
    fn close_on_an_agent_pane_asks_through_close_agent() {
        let (mut app, _rx, _dirs) = app_with(1);
        let pane = agent(&mut app, 0, 0);
        opened(&mut app);
        app.processes_act(Action::ClosePane(pane));
        assert!(matches!(app.overlay, Some(Overlay::CloseAgent(_))), "{:?}", app.overlay.as_ref().map(Overlay::name));
    }
}
