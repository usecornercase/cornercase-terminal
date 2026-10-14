use std::time::Instant;

use super::run::{Kind, MAX_MESSAGES, Member, Message, Run};
use super::store::{self, Origin};
use super::{Back, End, Flow, ORCHESTRATOR, Role, Stage, branch, builtin, command, resume};
use crate::agents::Known;
use crate::config::Config;
use crate::test_util::TempDir;

fn sample() -> Flow {
    let mut flows = builtin::flows(&Known::ALL, Some(Known::Claude));
    flows.remove(0)
}

mod builtins {
    use super::*;

    #[test]
    fn every_built_in_flow_is_valid_whatever_is_installed() {
        let sets: [&[Known]; 4] = [&Known::ALL, &[Known::Codex], &[], &[Known::Gemini, Known::Opencode]];
        for installed in sets {
            for default in [None, Some(Known::Claude), Some(Known::Codex)] {
                for flow in builtin::flows(installed, default) {
                    assert_eq!(flow.problems(), Vec::<String>::new(), "{} with {installed:?}", flow.name);
                }
            }
        }
    }

    #[test]
    fn the_default_agent_orchestrates_and_reviewers_use_other_agents() {
        let flow = builtin::flows(&[Known::Claude, Known::Codex, Known::Gemini], Some(Known::Codex)).remove(0);
        assert_eq!(flow.orchestrator.agent, "codex");
        assert_eq!(flow.role("coder").map(|r| r.agent.as_str()), Some("codex"));
        assert_eq!(flow.role("reviewer").map(|r| r.agent.as_str()), Some("claude"));
        assert_eq!(flow.role("security").map(|r| r.agent.as_str()), Some("gemini"));
    }

    #[test]
    fn with_one_agent_installed_every_role_uses_it() {
        let flow = builtin::flows(&[Known::Gemini], Some(Known::Claude)).remove(0);
        assert!(flow.roles.iter().all(|role| role.agent == "gemini"));
        assert_eq!(flow.orchestrator.agent, "gemini");
    }
}

mod problems {
    use super::*;

    #[test]
    fn a_stage_can_only_go_back_to_an_earlier_one() {
        let mut flow = sample();
        flow.stages[0].back = Some(Back { to: 1, ..Back::default() });
        assert!(flow.problems().contains(&"stage 1 can only go back to an earlier stage".to_string()));
    }

    #[test]
    fn roles_need_a_known_agent_a_unique_name_and_a_stage() {
        let mut flow = sample();
        flow.roles.push(Role { name: "coder".into(), agent: "vim".into(), ..Role::default() });
        let problems = flow.problems();
        assert!(problems.contains(&"two roles are named `coder`".to_string()), "{problems:?}");
        assert!(problems.iter().any(|p| p.starts_with("`coder` runs `vim`")), "{problems:?}");
        flow.roles.pop();
        flow.roles.push(Role { name: "idle".into(), agent: "codex".into(), ..Role::default() });
        assert_eq!(flow.problems(), vec!["`idle` is in no stage".to_string()]);
    }

    #[test]
    fn a_role_works_in_one_stage() {
        let mut flow = sample();
        flow.stages.push(Stage { roles: vec!["coder".into()], back: None });
        assert_eq!(flow.problems(), vec!["`coder` is in stage 1 and stage 4; a role works in one stage".to_string()]);
    }

    #[test]
    fn a_stage_must_name_existing_roles() {
        let mut flow = sample();
        flow.stages.push(Stage { roles: vec!["ghost".into()], back: None });
        assert_eq!(flow.problems(), vec!["stage 4 names `ghost`, which is not a role".to_string()]);
    }

    #[test]
    fn env_keys_must_be_variable_names() {
        let mut flow = sample();
        flow.roles[0].env.insert("NOT OK".into(), "1".into());
        assert_eq!(flow.problems(), vec!["`coder` sets `NOT OK`, which is not a variable name".to_string()]);
    }
}

mod files {
    use super::*;

    #[test]
    fn a_flow_survives_a_round_trip_through_toml() {
        for flow in builtin::flows(&Known::ALL, None) {
            let text = store::to_text(&flow).expect("serialize");
            assert_eq!(store::parse(&text).expect("parse"), flow);
        }
    }

    #[test]
    fn a_hand_written_flow_reads_with_defaults() {
        let text = r#"
name = "Mine"
end = "commits"

[orchestrator]
agent = "claude"
prompt = "Be brief."

[[role]]
name = "coder"
agent = "codex"
model = "gpt-5.5"
edits = true
prompt = """
Implement it.
Commit locally.
"""

[[stage]]
roles = ["coder"]
"#;
        let flow = store::parse(text).expect("parse");
        assert_eq!(flow.end, End::Commits);
        assert_eq!(flow.orchestrator.name, ORCHESTRATOR);
        assert_eq!(flow.roles[0].model.as_deref(), Some("gpt-5.5"));
        assert_eq!(flow.roles[0].prompt, "Implement it.\nCommit locally.\n");
        assert!(flow.problems().is_empty(), "{:?}", flow.problems());
    }

    #[test]
    fn a_flow_from_a_newer_cornercase_is_refused() {
        assert!(store::parse("version = 99\nname = \"x\"").is_err());
    }

    #[test]
    fn saving_names_the_file_after_the_flow_and_renames_it_with_the_flow() {
        let dir = TempDir::new();
        let mut flow = sample();
        let path = store::save(dir.path(), &flow, None).expect("save");
        assert_eq!(path, dir.path().join("full-issue.toml"));
        let other = store::save(dir.path(), &flow, None).expect("save a second one");
        assert_eq!(other, dir.path().join("full-issue-2.toml"));
        flow.name = "Renamed flow!".into();
        let renamed = store::save(dir.path(), &flow, Some(&path)).expect("rename");
        assert_eq!(renamed, dir.path().join("renamed-flow.toml"));
        assert!(!path.exists());
        let listing = store::list(Vec::new(), dir.path(), None);
        let names: Vec<_> = listing.flows.iter().map(|saved| saved.flow.name.as_str()).collect();
        assert_eq!(names, ["Full issue", "Renamed flow!"]);
        assert!(listing.flows.iter().all(|saved| matches!(saved.origin, Origin::User(_))));
        store::delete(&renamed).expect("delete");
        assert!(!renamed.exists());
    }

    #[test]
    fn listing_puts_built_in_flows_first_then_yours_then_the_projects_and_reports_broken_files() {
        let user = TempDir::new();
        let project = TempDir::new();
        store::save(user.path(), &sample(), None).expect("save");
        let dir = store::project_dir(project.path());
        store::save(&dir, &sample(), None).expect("save");
        std::fs::write(dir.join("broken.toml"), "name = ").expect("write");
        let listing = store::list(vec![sample()], user.path(), Some(&dir));
        let origins: Vec<_> = listing.flows.iter().map(|saved| saved.origin.label()).collect();
        assert_eq!(origins, ["built in", "yours", "this project"]);
        assert_eq!(listing.broken.len(), 1);
        assert_eq!(listing.broken[0].0, dir.join("broken.toml"));
    }

    #[test]
    fn file_names_are_slugs() {
        assert_eq!(store::file_name("Bug, test first"), "bug-test-first");
        assert_eq!(store::file_name("  ¡Revisión!  "), "revisión");
        assert_eq!(store::file_name("***"), "flow");
    }
}

mod commands {
    use super::*;

    #[test]
    fn a_role_starts_without_asking_with_its_model_its_arguments_and_its_environment() {
        let mut config = Config::default();
        config.agent_args.insert("claude".into(), vec!["--dangerously-skip-permissions".into(), "--verbose".into()]);
        let role = Role {
            name: "coder".into(),
            agent: "claude".into(),
            model: Some("opus".into()),
            args: vec!["--add-dir".into(), "../docs".into()],
            env: [("CLAUDE_CODE_USE_BEDROCK".to_string(), "1".to_string())].into(),
            ..Role::default()
        };
        assert_eq!(
            command(&config, &role).as_deref(),
            Some(
                "env CLAUDE_CODE_USE_BEDROCK=1 claude --dangerously-skip-permissions --verbose --model opus --add-dir \
                 ../docs"
            )
        );
    }

    #[test]
    fn a_model_in_the_settings_gives_way_to_the_roles() {
        let mut config = Config::default();
        config.agent_args.insert("codex".into(), vec!["--model".into(), "o3".into(), "--search".into()]);
        let role = Role { agent: "codex".into(), model: Some("gpt-5.5".into()), ..Role::default() };
        assert_eq!(
            command(&config, &role).as_deref(),
            Some("codex --dangerously-bypass-approvals-and-sandbox --search --model gpt-5.5")
        );
    }

    #[test]
    fn a_role_resumes_with_its_mode_model_and_environment() {
        let role = Role {
            agent: "claude".into(),
            model: Some("opus".into()),
            env: [("CLAUDE_CODE_USE_BEDROCK".to_string(), "1".to_string())].into(),
            ..Role::default()
        };
        assert_eq!(
            resume(&Config::default(), &role, "c1".into()).as_deref(),
            Some("env CLAUDE_CODE_USE_BEDROCK=1 claude --dangerously-skip-permissions --model opus --resume c1")
        );
        assert_eq!(resume(&Config::default(), &Role { agent: "opencode".into(), ..Role::default() }, "c".into()), None);
    }

    #[test]
    fn the_branch_is_a_slug_of_the_tasks_first_words() {
        assert_eq!(
            branch("Fix the login form, then add a test for the empty password"),
            "fix-the-login-form-then-add-a-test"
        );
        assert_eq!(branch("¿Qué?"), "que");
        assert_eq!(branch("!!!"), "flow");
    }

    #[test]
    fn every_known_agent_has_a_mode_that_never_asks() {
        for kind in Known::ALL {
            let role = Role { agent: kind.name().into(), ..Role::default() };
            let line = command(&Config::default(), &role).expect("known agent");
            assert!(line.starts_with(kind.name()), "{line}");
            assert!(line.len() > kind.name().len(), "{line}");
        }
        assert_eq!(command(&Config::default(), &Role { agent: "vim".into(), ..Role::default() }), None);
    }
}

mod runs {
    use super::*;

    #[test]
    fn a_run_maps_roles_to_panes_and_keeps_the_last_messages() {
        let mut run = Run::new(7, sample(), "fix it".into());
        run.members.push(Member { role: ORCHESTRATOR.into(), pane: 10 });
        run.members.push(Member { role: "coder".into(), pane: 11 });
        assert_eq!(run.role_of(11), Some("coder"));
        assert_eq!(run.pane_of(ORCHESTRATOR), Some(10));
        assert_eq!(run.role_of(99), None);
        let now = Instant::now();
        for n in 0..=MAX_MESSAGES {
            run.record(Message {
                from: ORCHESTRATOR.into(),
                to: "coder".into(),
                kind: Kind::Send,
                text: Some(n.to_string()),
                bytes: 1,
                at: now,
            });
        }
        assert_eq!(run.messages.len(), MAX_MESSAGES);
        assert_eq!(run.messages.front().and_then(|m| m.text.as_deref()), Some("1"));
    }
}
