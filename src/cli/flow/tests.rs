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

#[test]
fn the_default_flow_is_tagged_in_the_list() {
    let quick = FlowInfo { name: "Quick".into(), origin: "built-in".into(), default: true, ..FlowInfo::default() };
    let mine = FlowInfo {
        name: "Mine".into(),
        origin: "user".into(),
        path: Some("/c/flows/mine.toml".into()),
        default: true,
        ..FlowInfo::default()
    };
    let text = listing(&Flows { flows: vec![quick, mine], ..Flows::default() });
    assert!(text.starts_with("Quick (built in, default)\n"), "{text}");
    assert!(text.contains("Mine (yours, default, /c/flows/mine.toml)\n"), "{text}");
}

#[test]
fn setting_the_default_says_what_changed() {
    let set = |name: Option<&str>, changed| FlowDefaultSet { name: name.map(Into::into), changed };
    assert_eq!(default_line(&set(Some("Quick"), true)), "the start dialog opens on Quick now\n");
    assert_eq!(default_line(&set(Some("Quick"), false)), "Quick was already the default flow\n");
    assert_eq!(
        default_line(&set(None, true)),
        "no default flow: the start dialog opens on the first built-in flow that can start\n"
    );
    assert_eq!(default_line(&set(None, false)), "there was no default flow\n");
}

#[test]
fn ordering_says_the_order_the_flows_are_in() {
    let names = vec!["Quick".to_string(), "Full issue".to_string()];
    let changed = FlowOrdered { names: names.clone(), changed: true };
    assert_eq!(ordered_line(&changed), "the flows are in this order now: Quick, Full issue\n");
    let same = FlowOrdered { names, changed: false };
    assert_eq!(ordered_line(&same), "the flows were already in this order: Quick, Full issue\n");
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
    fn a_dropped_frame_is_told_and_not_read_as_a_message() {
        let frame = serde_json::json!({"event": "dropped", "count": 3});

        assert_eq!(
            frame_text(frame.clone(), false).expect("text"),
            "3 messages dropped: this command fell behind; run it again to read what is kept\n"
        );
        assert!(
            frame_text(serde_json::json!({"event": "dropped", "count": 1}), false)
                .expect("text")
                .starts_with("1 message dropped")
        );
        assert_eq!(frame_text(frame, true).expect("json"), "{\"count\":3,\"event\":\"dropped\"}\n");
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

mod saving {
    use super::*;
    use crate::control::RoleSpec;

    fn saved(path: Option<&str>, origin: &str, problems: &[&str]) -> FlowSaved {
        FlowSaved {
            name: "Mine".into(),
            origin: origin.into(),
            saved: path.is_some(),
            path: path.map(Into::into),
            problems: problems.iter().map(ToString::to_string).collect(),
            check: None,
        }
    }

    #[test]
    fn a_save_names_its_place_and_its_file_then_what_still_stops_the_flow() {
        assert_eq!(
            saved_text(&saved(Some("/c/flows/mine.toml"), "user", &[])),
            "saved Mine into yours: /c/flows/mine.toml\n"
        );
        let clash = "yours is also named `Mine` and runs instead";
        let project = FlowSaved {
            check: Some("it sets A for `coder`".into()),
            ..saved(Some("/r/.cornercase/flows/mine.toml"), "project", &[clash])
        };
        assert_eq!(
            saved_text(&project),
            "saved Mine into this project: /r/.cornercase/flows/mine.toml\n  cannot run: yours is also named `Mine` and runs instead\n  it sets A for `coder`\n"
        );
    }

    #[test]
    fn a_check_says_nothing_was_written_and_lists_the_problems() {
        assert_eq!(saved_text(&saved(None, "user", &[])), "Mine can be saved into yours; nothing was written\n");
        assert_eq!(
            saved_text(&saved(None, "user", &["the flow has no stages"])),
            "Mine has problems; nothing was written\n  cannot run: the flow has no stages\n"
        );
    }

    #[test]
    fn a_draft_is_a_flow_as_the_list_gives_it_and_may_leave_keys_out() {
        let spec = FlowSpec {
            name: "Mine".into(),
            roles: vec![RoleSpec { name: "coder".into(), agent: "claude".into(), edits: true, ..RoleSpec::default() }],
            ..FlowSpec::default()
        };
        assert_eq!(parsed(&serde_json::to_string(&spec).expect("json")), Ok(spec));
        assert_eq!(parsed(r#"{"name": "Mine"}"#), Ok(FlowSpec { name: "Mine".into(), ..FlowSpec::default() }));
    }

    #[test]
    fn a_key_a_flow_does_not_have_is_refused_with_its_place() {
        let typo = parsed(r#"{"name":"Mine","roles":[{"name":"coder","promt":"x"}]}"#).expect_err("a stray key");
        assert!(typo.starts_with("`roles.0.promt` is not a key of a flow"), "{typo}");
        let entry = serde_json::to_string(&FlowInfo::default()).expect("json");
        assert!(parsed(&entry).expect_err("an entry").contains("jq '.flows[0].flow'"));
        assert!(parsed("[]").is_err());
        assert!(parsed("{").is_err());
    }

    #[test]
    fn a_path_is_made_absolute_without_dots_before_it_is_matched_with_a_listed_file() {
        assert_eq!(
            listed_path(Path::new("/c/flows/../flows/./mine.toml")).expect("a path"),
            Path::new("/c/flows/mine.toml")
        );
        let here = std::env::current_dir().expect("a folder");
        assert_eq!(listed_path(Path::new("a/../mine.toml")).expect("a path"), here.join("mine.toml"));
    }

    #[test]
    fn an_edit_stays_in_the_place_its_file_is_listed_in() {
        let entry = |origin: &str, path: &str| FlowInfo {
            origin: origin.into(),
            path: Some(path.into()),
            ..FlowInfo::default()
        };
        let flows =
            Flows { flows: vec![entry("user", "/c/a.toml"), entry("project", "/r/b.toml")], ..Flows::default() };
        assert_eq!(listed_place(&flows, Path::new("/c/a.toml")), Some(ImportTo::Yours));
        assert_eq!(listed_place(&flows, Path::new("/r/b.toml")), Some(ImportTo::Project));
        assert_eq!(listed_place(&flows, Path::new("/x.toml")), None);
    }
}
