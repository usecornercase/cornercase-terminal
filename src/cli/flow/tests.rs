use super::*;

fn removed(changed: bool, deleted: Option<&str>) -> FlowRemoved {
    FlowRemoved {
        name: "Quick".into(),
        origin: Some("user".into()),
        deleted: deleted.map(Into::into),
        hidden: false,
        changed,
    }
}

#[test]
fn a_deleted_flow_names_its_place_and_its_file() {
    assert_eq!(deleted_line(&removed(true, Some("/c/flows/mine.toml"))), "deleted Quick (yours): /c/flows/mine.toml\n");
    let project = FlowRemoved { origin: Some("project".into()), ..removed(true, None) };
    assert_eq!(deleted_line(&project), "deleted Quick (this project's)\n");
}

#[test]
fn hiding_says_how_to_bring_the_flow_back_and_what_else_went() {
    assert_eq!(
        hidden_line(&removed(true, None)),
        "deleted the built-in flow Quick; `cornercase flow restore Quick` brings it back\n"
    );
    assert!(
        hidden_line(&removed(true, Some("/c/flows/quick.toml"))).contains(", and your version /c/flows/quick.toml;")
    );
    assert_eq!(hidden_line(&removed(false, None)), "the built-in flow Quick was already deleted\n");
}

#[test]
fn restoring_says_what_was_deleted_or_that_there_was_nothing() {
    assert_eq!(
        restored_line(&removed(true, Some("/c/flows/quick.toml"))),
        "restored the built-in flow Quick; your version /c/flows/quick.toml is deleted\n"
    );
    assert_eq!(restored_line(&removed(true, None)), "restored the built-in flow Quick\n");
    assert!(restored_line(&removed(false, None)).contains("nothing to restore"));
}

#[test]
fn the_list_ends_with_the_deleted_built_ins_when_there_are_some() {
    let hidden = FlowInfo { name: "Quick".into(), origin: "built-in".into(), ..FlowInfo::default() };
    let flows = Flows { hidden: vec![hidden], ..Flows::default() };
    assert_eq!(listing(&flows), "deleted built-in flows: Quick (`cornercase flow restore NAME` brings one back)\n");
    assert_eq!(listing(&Flows::default()), "");
}

mod runs {
    use super::*;
    use crate::control::{BackSpec, FlowSpec, RoleSpec, RoleState, RunFile, StageSpec, StepInfo};

    fn spec() -> FlowSpec {
        let role = |name: &str| RoleSpec { name: name.into(), agent: "claude".into(), ..RoleSpec::default() };
        let stage = |roles: &[&str], back: Option<BackSpec>| StageSpec {
            roles: roles.iter().map(ToString::to_string).collect(),
            back,
        };
        FlowSpec {
            name: "Full issue".into(),
            orchestrator: role("orchestrator"),
            roles: vec![role("coder"), role("reviewer"), role("tester")],
            stages: vec![
                stage(&["coder"], None),
                stage(&["reviewer"], Some(BackSpec { to: 1, when: "findings".into(), at_most: 3 })),
                stage(&["tester"], None),
                stage(&[], None),
            ],
            ..FlowSpec::default()
        }
    }

    fn state(role: &str, pane: Option<u64>, status: Option<&str>, closed: bool) -> RoleState {
        RoleState { role: role.into(), pane, status: status.map(str::to_string), closed, ..RoleState::default() }
    }

    fn run() -> RunState {
        RunState {
            workspace: 4,
            flow: spec(),
            task: "Fix the login form\nand the rest".into(),
            roles: vec![
                state("orchestrator", Some(12), Some("working"), false),
                state("coder", Some(13), Some("idle"), false),
                state("reviewer", Some(14), None, true),
                state("tester", None, None, false),
            ],
            stage: Some(1),
            rounds: vec![0, 1, 0, 0],
            holder: Some("reviewer".into()),
            last: Some(StepInfo {
                edge: RunEdge::Forward { stage: 0 },
                from: Some("coder".into()),
                to: "reviewer".into(),
                time: "2026-10-18T09:30:00.000Z".into(),
            }),
            outside: Some(RunEdge::Outside { from: Some(0), to: 2 }),
            ended: true,
            ..RunState::default()
        }
    }

    #[test]
    fn a_run_is_a_block_of_its_stage_its_roles_and_its_steps() {
        let text = runs_text(&FlowRuns { runs: vec![run()], ..FlowRuns::default() });

        assert_eq!(
            text,
            "workspace 4  Full issue  stage 2 of 4  findings 1/3\n  \
             task: Fix the login form\n  \
             orchestrator  pane 12    working\n  \
             coder         pane 13    idle\n  \
             reviewer      pane 14    closed\n  \
             tester        -          -\n  \
             last: coder → reviewer\n  \
             outside the plan: stage 1 → stage 3\n  \
             ended: the orchestrator marked its pane for review\n"
        );
    }

    #[test]
    fn a_run_that_has_not_started_says_so_and_nothing_running_says_that() {
        let fresh = RunState { stage: None, rounds: Vec::new(), last: None, outside: None, ended: false, ..run() };

        let text = runs_text(&FlowRuns { runs: vec![fresh], ..FlowRuns::default() });

        assert!(text.starts_with("workspace 4  Full issue  not started\n"), "{text}");
        assert!(!text.contains("last:") && !text.contains("ended"), "{text}");
        assert_eq!(runs_text(&FlowRuns::default()), "no flow is running\n");
    }

    #[test]
    fn several_runs_are_separated_by_a_blank_line() {
        let text = runs_text(&FlowRuns { runs: vec![run(), run()], ..FlowRuns::default() });

        assert_eq!(text.matches("\n\nworkspace 4").count(), 1, "{text}");
    }

    fn message(kind: &str, text: Option<&str>, bytes: usize) -> FlowMessage {
        FlowMessage {
            time: "2026-10-18T09:30:00.000Z".into(),
            from: "orchestrator".into(),
            to: "coder".into(),
            kind: kind.into(),
            text: text.map(str::to_string),
            bytes,
            ..FlowMessage::default()
        }
    }

    #[test]
    fn a_message_is_a_heading_and_its_indented_text_without_control_characters() {
        let sent = message("send", Some("fix it\x1b[2J\n\tthen test"), 20);

        assert_eq!(
            message_text(&sent, false),
            "2026-10-18T09:30:00.000Z  orchestrator → coder  sent  20 bytes\n  fix it [2J\n  \tthen test\n"
        );
        assert_eq!(
            message_text(&message("read", None, 0), false),
            "2026-10-18T09:30:00.000Z  orchestrator → coder  read  0 bytes\n"
        );
        assert!(message_text(&message("wait", None, 1), false).contains("waited for  1 byte\n"));
        assert!(message_text(&message("keys", Some("enter"), 5), false).contains("keys pressed"));
    }

    #[test]
    fn json_is_one_raw_message_per_line() {
        let sent = message("send", Some("a\nb"), 3);

        let line = message_text(&sent, true);

        assert!(line.ends_with('\n') && line.matches('\n').count() == 1, "{line}");
        let back: FlowMessage = serde_json::from_str(&line).expect("a message");
        assert_eq!(back, sent);
    }

    #[test]
    fn files_are_a_tab_separated_table_with_a_header_and_dashes_for_gaps() {
        let files = RunFiles {
            files: vec![
                RunFile {
                    name: "coder-1.md".into(),
                    path: "/r/coder-1.md".into(),
                    bytes: 12,
                    modified: Some("2026-10-18T09:30:00.000Z".into()),
                },
                RunFile { name: "a\tb.md".into(), path: "/r/a\tb.md".into(), bytes: 0, modified: None },
            ],
            ..RunFiles::default()
        };

        assert_eq!(
            files_text(&files),
            "name\tbytes\tmodified\tpath\ncoder-1.md\t12\t2026-10-18T09:30:00.000Z\t/r/coder-1.md\na b.md\t0\t-\t/r/a b.md\n"
        );
    }
}
