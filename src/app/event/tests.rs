use super::*;
use crate::machines::stream::{StreamEvent, StreamReport};

#[test]
fn a_machine_tab_s_answer_and_a_machine_panel_s_line_echo_a_flood() {
    let answer =
        AppEvent::Machine(MachineEvent::Stream(StreamReport { stream: 1, event: StreamEvent::Down("x".into()) }));
    let line = AppEvent::a_machine_panel_line();

    assert!(answer.echoes_a_flood() && line.echoes_a_flood());
}

#[test]
fn another_job_answer_does_not() {
    let repo = AppEvent::Machine(MachineEvent::Repo { project: 1, repo: None });

    assert!(!repo.echoes_a_flood() && !AppEvent::Input(Event::FocusGained).echoes_a_flood());
}
