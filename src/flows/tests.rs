use std::time::Instant;

use rstest::rstest;

use super::run::{Kind, MAX_MESSAGES, Member, Message, Run};
use super::store::{self, Origin};
use super::{Back, End, Flow, ORCHESTRATOR, Role, Stage, Worktree, branch, builtin, command, resume, role_branch};
use crate::agents::Known;
use crate::config::Config;
use crate::test_util::TempDir;

fn sample() -> Flow {
    let mut flows = builtin::flows(&Known::ALL, Some(Known::Claude));
    flows.remove(0)
}

mod builtins {
    use super::*;

    fn agents_of(flow: &Flow) -> Vec<(&str, &str)> {
        std::iter::once(&flow.orchestrator).chain(&flow.roles).map(|r| (r.name.as_str(), r.agent.as_str())).collect()
    }

    fn every_set() -> Vec<Vec<Known>> {
        (0..1 << Known::ALL.len())
            .map(|bits: u32| {
                Known::ALL.into_iter().enumerate().filter(|(i, _)| bits & (1 << i) != 0).map(|(_, k)| k).collect()
            })
            .collect()
    }

    #[test]
    fn every_built_in_flow_is_valid_whenever_the_installed_agents_allow_it() {
        for installed in every_set() {
            for default in std::iter::once(None).chain(Known::ALL.map(Some)) {
                let flows =
                    builtin::flows(&installed, default).into_iter().chain([builtin::starter(&installed, default)]);
                for flow in flows {
                    let possible =
                        installed.iter().any(|kind| !kind.status_by_folder()) || installed.len() >= flow.agents();
                    let problems = flow.problems();
                    let others: Vec<&String> = problems.iter().filter(|p| !p.contains("cannot tell two")).collect();
                    let what = format!("{} with {installed:?} and {default:?}: {problems:?}", flow.name);
                    assert!(others.is_empty(), "{what}");
                    assert!(!possible || problems.is_empty(), "{what}");
                }
            }
        }
    }

    #[test]
    fn codex_and_gemini_take_several_roles() {
        let flow = builtin::flows(&[Known::Claude, Known::Codex, Known::Gemini], Some(Known::Codex)).remove(0);
        assert_eq!(
            agents_of(&flow),
            [
                ("orchestrator", "codex"),
                ("coder", "codex"),
                ("reviewer", "claude"),
                ("security", "gemini"),
                ("tester", "codex")
            ]
        );
        let flow = builtin::flows(&[Known::Gemini], None).remove(2);
        assert!(agents_of(&flow).iter().all(|(_, agent)| *agent == "gemini"));
        assert_eq!(flow.problems(), Vec::<String>::new());
    }

    #[test]
    fn opencode_codes_and_an_agent_that_shares_a_folder_takes_the_other_roles() {
        let flow = builtin::flows(&[Known::Claude, Known::Codex, Known::Opencode], Some(Known::Opencode)).remove(0);
        assert_eq!(
            agents_of(&flow),
            [
                ("orchestrator", "claude"),
                ("coder", "opencode"),
                ("reviewer", "claude"),
                ("security", "codex"),
                ("tester", "claude")
            ]
        );
    }

    #[test]
    fn the_default_agent_orchestrates_when_it_can_share_a_folder() {
        let flow = builtin::flows(&[Known::Claude, Known::Codex, Known::Gemini], Some(Known::Claude)).remove(0);
        assert_eq!(
            agents_of(&flow),
            [
                ("orchestrator", "claude"),
                ("coder", "claude"),
                ("reviewer", "codex"),
                ("security", "gemini"),
                ("tester", "claude")
            ]
        );
    }

    #[test]
    fn with_only_opencode_installed_every_role_uses_it_and_says_why_it_cannot_run() {
        let flow = builtin::flows(&[Known::Opencode], Some(Known::Claude)).remove(2);
        assert!(agents_of(&flow).iter().all(|(_, agent)| *agent == "opencode"));
        assert_eq!(
            flow.problems(),
            ["opencode runs `orchestrator`, `coder` and `reviewer`, but cornercase reads opencode's status from its \
                 folder and cannot tell two apart in one worktree: give all but one of them another agent, or a worktree of its own"]
        );
    }

    fn arena(installed: &[Known], default: Known) -> Flow {
        builtin::flows(installed, Some(default)).pop().expect("flows")
    }

    #[test]
    fn arena_is_the_last_built_in_and_every_set_of_agents_runs_it() {
        for installed in every_set().into_iter().filter(|installed| !installed.is_empty()) {
            let flow = arena(&installed, installed[0]);
            assert_eq!(flow.name, "Arena");
            let possible = installed.iter().any(|kind| !kind.status_by_folder()) || installed.len() >= 3;
            assert!(!possible || flow.problems().is_empty(), "{installed:?}: {:?}", flow.problems());
        }
    }

    #[test]
    fn arena_gives_each_competing_coder_an_agent_and_a_worktree_of_its_own() {
        let flow = arena(&Known::ALL, Known::Claude);
        let coders: Vec<(&str, &str, bool)> = flow
            .roles
            .iter()
            .filter(|r| r.own_worktree())
            .map(|r| (r.name.as_str(), r.agent.as_str(), r.edits))
            .collect();
        assert_eq!(coders, [("coder A", "claude", true), ("coder B", "codex", true), ("coder C", "gemini", true)]);
        assert_eq!(flow.stages[0].roles, ["coder A", "coder B", "coder C"]);
        assert!(flow.roles.iter().filter(|r| !r.own_worktree()).all(|r| r.worktree == Worktree::Shared));

        let two = arena(&[Known::Claude, Known::Codex], Known::Claude);
        assert_eq!(two.stages[0].roles, ["coder A", "coder B"]);
        let one = arena(&[Known::Claude], Known::Claude);
        assert_eq!(one.stages[0].roles, ["coder A", "coder B"]);
        assert!(one.roles.iter().take(2).all(|role| role.agent == "claude"));
    }

    #[test]
    fn opencode_takes_several_roles_when_they_work_apart() {
        let flow = arena(&[Known::Opencode, Known::Claude], Known::Opencode);
        let own: Vec<&str> = flow.roles.iter().filter(|r| r.own_worktree()).map(|r| r.agent.as_str()).collect();
        assert_eq!(own, ["opencode", "claude"]);
        let shared: Vec<&str> = std::iter::once(&flow.orchestrator)
            .chain(&flow.roles)
            .filter(|r| !r.own_worktree())
            .map(|r| r.agent.as_str())
            .collect();
        assert_eq!(shared.iter().filter(|agent| **agent == "opencode").count(), 1, "{shared:?}");
        assert!(flow.problems().is_empty(), "{:?}", flow.problems());
    }

    #[test]
    fn a_new_role_takes_an_agent_that_shares_a_folder_when_opencode_is_taken() {
        let installed = [Known::Claude, Known::Opencode];
        let role = builtin::member("docs", &installed, Some(Known::Opencode), &[Known::Opencode]);
        assert_eq!(role.agent, "claude");
        let role = builtin::member("docs", &installed, Some(Known::Opencode), &[Known::Claude]);
        assert_eq!(role.agent, "opencode");
        let role = builtin::member("docs", &[Known::Codex], Some(Known::Codex), &[Known::Codex]);
        assert_eq!(role.agent, "codex");
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
        flow.roles.push(Role { name: "idle".into(), agent: "claude".into(), ..Role::default() });
        assert_eq!(flow.problems(), vec!["`idle` is in no stage".to_string()]);
    }

    #[test]
    fn an_effort_must_be_one_the_agent_takes() {
        let mut flow = sample();
        flow.roles[0].effort = Some("high".into());
        assert_eq!(flow.problems(), Vec::<String>::new());
        flow.roles[0].effort = Some("huge".into());
        let huge = "`coder`: claude has no effort `huge`; its levels are `low`, `medium`, `high`, `xhigh`, `max`";
        assert_eq!(flow.problems(), vec![huge.to_string()]);
        flow.roles[0].effort = None;
        flow.roles[2].agent = "gemini".into();
        flow.roles[2].effort = Some("high".into());
        assert!(
            flow.problems().contains(&"`security`: gemini has no effort switch".to_string()),
            "{:?}",
            flow.problems()
        );
    }

    #[test]
    fn a_role_works_in_one_stage() {
        let mut flow = sample();
        flow.stages.push(Stage { roles: vec!["coder".into()], back: None });
        assert_eq!(flow.problems(), vec!["`coder` is in stage 1 and stage 4; a role works in one stage".to_string()]);
    }

    #[rstest::rstest]
    #[case::codex("codex", false)]
    #[case::gemini("gemini", false)]
    #[case::opencode("opencode", true)]
    #[case::claude("claude", false)]
    #[case::grok("grok", false)]
    #[case::cursor("cursor", false)]
    fn only_opencode_takes_one_role(#[case] agent: &str, #[case] refused: bool) {
        let mut flow = sample();
        flow.orchestrator.agent = agent.into();
        flow.roles[2].agent = agent.into();
        let problems: Vec<String> = flow.problems().into_iter().filter(|p| p.contains("cannot tell")).collect();
        assert_eq!(problems.len(), usize::from(refused), "{problems:?}");
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

mod worktrees {
    use super::*;

    fn pair() -> Flow {
        let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(2);
        flow.roles = vec![
            Role { name: "a".into(), agent: "claude".into(), edits: true, ..Role::default() },
            Role { name: "b".into(), agent: "codex".into(), edits: true, ..Role::default() },
        ];
        flow.stages = vec![Stage { roles: vec!["a".into(), "b".into()], back: None }];
        flow
    }

    #[test]
    fn two_roles_that_edit_one_worktree_side_by_side_are_a_problem() {
        let flow = pair();
        assert_eq!(
            flow.problems(),
            ["stage 1 runs `a` and `b` side by side, and they both edit the shared worktree: give all but one of \
              them a worktree of its own, or put them in stages of their own"]
        );
    }

    #[rstest::rstest]
    #[case::one_apart(true, false, true, false)]
    #[case::both_apart(true, true, true, true)]
    #[case::one_reads(false, false, true, false)]
    fn the_problem_only_applies_to_roles_that_share_and_edit(
        #[case] first_own: bool,
        #[case] second_own: bool,
        #[case] first_edits: bool,
        #[case] second_edits: bool,
    ) {
        let mut flow = pair();
        flow.roles[0].worktree = if first_own { Worktree::Own } else { Worktree::Shared };
        flow.roles[1].worktree = if second_own { Worktree::Own } else { Worktree::Shared };
        flow.roles[0].edits = first_edits;
        flow.roles[1].edits = second_edits;
        assert_eq!(flow.problems(), Vec::<String>::new());
    }

    #[test]
    fn the_orchestrator_works_in_the_runs_worktree() {
        let mut flow = pair();
        flow.roles[0].worktree = Worktree::Own;
        flow.orchestrator.worktree = Worktree::Own;
        assert_eq!(
            flow.problems(),
            ["the orchestrator works in the run's worktree: only a role can have a worktree of its own"]
        );
    }

    #[test]
    fn two_roles_whose_names_make_one_branch_are_a_problem() {
        let mut flow = pair();
        flow.roles[0].name = "Coder A".into();
        flow.roles[1].name = "coder a".into();
        flow.stages[0].roles = vec!["Coder A".into(), "coder a".into()];
        flow.roles.iter_mut().for_each(|role| role.worktree = Worktree::Own);
        assert_eq!(flow.problems(), ["`Coder A` and `coder a` would work on the same branch: rename one"]);
    }

    #[test]
    fn opencode_takes_several_roles_that_work_apart_but_not_in_one_worktree() {
        let mut flow = pair();
        flow.orchestrator.agent = "claude".into();
        flow.roles.iter_mut().for_each(|role| "opencode".clone_into(&mut role.agent));
        flow.roles[0].worktree = Worktree::Own;
        assert_eq!(flow.problems(), Vec::<String>::new());
        flow.roles[1].worktree = Worktree::Shared;
        flow.stages =
            vec![Stage { roles: vec!["a".into()], back: None }, Stage { roles: vec!["b".into()], back: None }];
        flow.orchestrator.agent = "opencode".into();
        assert_eq!(flow.problems().len(), 1, "{:?}", flow.problems());
    }

    #[test]
    fn a_role_branch_is_the_runs_branch_and_the_roles_slug() {
        assert_eq!(role_branch("fix-login", "coder A"), "fix-login--coder-a");
        assert_eq!(role_branch("fix", "***"), "fix--role");
        assert_eq!(role_branch("", "coder A"), "--coder-a");
    }
}

mod files {
    use super::*;

    fn own() -> Flow {
        let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).pop().expect("arena");
        assert_eq!(flow.name, "Arena");
        flow.version = super::super::VERSION;
        flow
    }

    #[test]
    fn a_file_says_version_two_only_when_a_role_has_a_worktree_of_its_own() {
        let text = store::to_text(&own()).expect("serialize");
        assert!(text.starts_with("version = 2\n"), "{text}");
        assert_eq!(text.matches("worktree = \"own\"").count(), 3, "{text}");
        assert_eq!(store::parse(&text).expect("parse"), own());
        let plain = store::to_text(&sample()).expect("serialize");
        assert!(plain.starts_with("version = 1\n"), "{plain}");
        assert!(!plain.contains("worktree = "), "{plain}");
        assert_eq!(store::parse(&plain).expect("parse"), sample());
    }

    #[test]
    fn an_own_worktree_without_version_two_is_refused_with_its_reason() {
        let text = store::to_text(&own()).expect("serialize").replacen("version = 2", "version = 1", 1);
        let refused = store::parse(&text).expect_err("refused");
        assert!(refused.contains("`coder A`") && refused.contains("version = 2"), "{refused}");
        let text = store::to_text(&own()).expect("serialize").replacen("version = 2\n", "", 1);
        assert!(store::parse(&text).is_err());
    }

    #[test]
    fn an_unknown_worktree_value_is_refused_with_its_line() {
        let text = store::to_text(&own()).expect("serialize").replacen("worktree = \"own\"", "worktree = \"mine\"", 1);
        let refused = store::parse(&text).expect_err("refused");
        assert!(refused.starts_with("line "), "{refused}");
    }

    #[test]
    fn a_flow_survives_a_round_trip_through_toml() {
        for flow in builtin::flows(&Known::ALL, None) {
            let text = store::to_text(&flow).expect("serialize");
            assert!(!text.contains("effort"), "{text}");
            assert_eq!(store::parse(&text).expect("parse"), flow);
        }
        let mut flow = sample();
        flow.roles[0].effort = Some("xhigh".into());
        let text = store::to_text(&flow).expect("serialize");
        assert!(text.contains("effort = \"xhigh\""), "{text}");
        assert_eq!(store::parse(&text).expect("parse"), flow);
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
    fn yours_replaces_a_built_in_of_its_name_and_a_projects_comes_after_both() {
        let (user, project) = (TempDir::new(), TempDir::new());
        store::save(user.path(), &sample(), None).expect("save");
        let dir = store::project_dir(project.path());
        store::save(&dir, &sample(), None).expect("save");
        let quick = Flow { name: "Quick".into(), ..sample() };
        store::save(&dir, &quick, None).expect("save");
        std::fs::write(dir.join("broken.toml"), "name = ").expect("write");
        let builtins = builtin::flows(&Known::ALL, Some(Known::Claude));
        let listing = store::list(builtins, user.path(), Some(&dir));
        let listed: Vec<(&str, &str)> =
            listing.flows.iter().map(|saved| (saved.flow.name.as_str(), saved.origin.label())).collect();
        assert_eq!(
            listed,
            [
                ("Arena", "built in"),
                ("Bug, test first", "built in"),
                ("Cross review", "built in"),
                ("Full issue", "yours"),
                ("Full issue", "this project"),
                ("Quick", "built in"),
                ("Quick", "this project"),
            ]
        );
        let conflicts: Vec<Option<String>> = listing.flows.iter().map(|saved| listing.conflict(saved)).collect();
        assert_eq!(conflicts[4].as_deref(), Some("yours is also named `Full issue` and runs instead"));
        assert_eq!(conflicts[6].as_deref(), Some("the built-in one is also named `Quick` and runs instead"));
        assert!(store::edits_builtin(&listing.builtins, &listing.flows[3]));
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
            command(&config, &role, None).as_deref(),
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
            command(&config, &role, None).as_deref(),
            Some("codex --dangerously-bypass-approvals-and-sandbox --search --model gpt-5.5")
        );
    }

    #[rstest::rstest]
    #[case::a_model_beats_every_other(Some("gpt-5.5"), &["-m", "o4", "--search"], "--model gpt-5.5 --search")]
    #[case::args_beat_the_settings(None, &["--model=o4", "--search"], "--model=o4 --search")]
    #[case::the_settings_stay_otherwise(None, &["--search"], "-m o3 --search")]
    fn a_role_model_then_its_arguments_beat_the_settings_model(
        #[case] model: Option<&str>,
        #[case] args: &[&str],
        #[case] expected: &str,
    ) {
        let mut config = Config::default();
        config.agent_args.insert("codex".into(), vec!["-m".into(), "o3".into()]);
        let role = Role {
            agent: "codex".into(),
            model: model.map(str::to_string),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            ..Role::default()
        };
        let line = command(&config, &role, None).expect("a known agent");
        assert_eq!(line, format!("codex --dangerously-bypass-approvals-and-sandbox {expected}"));
    }

    #[rstest::rstest]
    #[case::an_effort_beats_every_other(Some("high"), &["--effort", "low", "-v"], "--effort high -v")]
    #[case::args_beat_the_settings(None, &["--effort=low", "-v"], "--effort=low -v")]
    #[case::the_settings_stay_otherwise(None, &["-v"], "--effort max -v")]
    fn a_role_effort_then_its_arguments_beat_the_settings_effort(
        #[case] effort: Option<&str>,
        #[case] args: &[&str],
        #[case] expected: &str,
    ) {
        let mut config = Config::default();
        config.agent_args.insert("claude".into(), vec!["--effort".into(), "max".into()]);
        let role = Role {
            agent: "claude".into(),
            effort: effort.map(str::to_string),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            ..Role::default()
        };
        let line = command(&config, &role, None).expect("a known agent");
        assert_eq!(line, format!("claude --dangerously-skip-permissions {expected}"));
        let resumed = resume(&config, &role, "c1".into(), None).expect("claude resumes");
        assert_eq!(resumed, format!("claude --dangerously-skip-permissions {expected} --resume c1"));
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
            resume(&Config::default(), &role, "c1".into(), None).as_deref(),
            Some("env CLAUDE_CODE_USE_BEDROCK=1 claude --dangerously-skip-permissions --model opus --resume c1")
        );
        assert_eq!(
            resume(&Config::default(), &Role { agent: "opencode".into(), ..Role::default() }, "c".into(), None),
            None
        );
    }

    #[rstest]
    #[case::claude(
        "claude",
        "claude --dangerously-skip-permissions --add-dir /r/d",
        "claude --dangerously-skip-permissions --add-dir /r/d --resume c1"
    )]
    #[case::codex(
        "codex",
        "codex --dangerously-bypass-approvals-and-sandbox --add-dir /r/d",
        "codex resume --dangerously-bypass-approvals-and-sandbox --add-dir /r/d c1"
    )]
    #[case::cursor(
        "cursor",
        "cursor-agent --force --trust --add-dir /r/d",
        "cursor-agent --force --trust --add-dir /r/d --resume c1"
    )]
    #[case::gemini("gemini", "gemini --yolo --skip-trust", "gemini --yolo --skip-trust --resume c1")]
    #[case::grok("grok", "grok --always-approve", "grok --always-approve --resume c1")]
    fn the_run_folder_is_the_last_argument(#[case] agent: &str, #[case] start: &str, #[case] resumed: &str) {
        let role = Role { agent: agent.into(), ..Role::default() };
        let dir = Some(std::path::Path::new("/r/d"));
        assert_eq!(command(&Config::default(), &role, dir).as_deref(), Some(start));
        assert_eq!(resume(&Config::default(), &role, "c1".into(), dir).as_deref(), Some(resumed));
    }

    #[test]
    fn opencode_gets_no_folder_flag() {
        let role = Role { agent: "opencode".into(), ..Role::default() };
        let dir = Some(std::path::Path::new("/r/d"));
        assert_eq!(command(&Config::default(), &role, dir), command(&Config::default(), &role, None));
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
            let line = command(&Config::default(), &role, None).expect("known agent");
            assert!(line.starts_with(kind.name()), "{line}");
            assert!(line.len() > kind.name().len(), "{line}");
        }
        assert_eq!(command(&Config::default(), &Role { agent: "vim".into(), ..Role::default() }, None), None);
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

mod site_copy {
    use std::path::Path;

    use super::*;
    use crate::flows::share;

    const UPDATE_ENV: &str = "CORNERCASE_UPDATE_BUILTIN_FLOWS";

    fn generated() -> String {
        let flows: Vec<serde_json::Value> = builtin::flows(&Known::ALL, Some(Known::Claude))
            .iter()
            .map(|flow| {
                let slug = store::file_name(&flow.name);
                serde_json::json!({ "slug": slug, "file": format!("{slug}.toml"), "text": share::to_share(flow).expect("export") })
            })
            .collect();
        format!("{}\n", serde_json::to_string_pretty(&serde_json::json!({ "flows": flows })).expect("json"))
    }

    #[test]
    fn the_website_lists_the_built_in_flows_as_they_are() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("site/src/data/builtin-flows.json");
        let wanted = generated();
        if std::env::var_os(UPDATE_ENV).is_some() {
            std::fs::write(&path, &wanted).expect("write the site's copy");
        }
        let found = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            found == wanted,
            "site/src/data/builtin-flows.json differs from the built-in flows: run `{UPDATE_ENV}=1 cargo test --locked site_copy` and commit it"
        );
    }

    #[test]
    fn it_has_every_built_in_flow_once_with_a_distinct_file() {
        let flows = builtin::flows(&Known::ALL, Some(Known::Claude));
        let mut slugs: Vec<String> = flows.iter().map(|flow| store::file_name(&flow.name)).collect();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), flows.len());
    }
}
