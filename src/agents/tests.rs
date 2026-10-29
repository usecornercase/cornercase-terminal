use std::collections::BTreeMap;

use rstest::rstest;

use super::*;

mod installed {
    use super::*;
    use crate::test_util::{TempDir, write_executable};

    fn bin() -> TempDir {
        let dir = TempDir::new();
        write_executable(&dir.path().join("codex"), "#!/bin/sh\n");
        std::fs::write(dir.path().join("notes"), "").expect("write a plain file");
        dir
    }

    #[test]
    fn a_program_is_looked_up_in_the_path() {
        let dir = bin();
        let path = std::env::join_paths(["/nonexistent", dir.path().to_str().expect("utf-8")]).expect("a path");
        assert_eq!(
            [found("codex", Some(&path)), found("claude", Some(&path)), found("notes", Some(&path))],
            [true, false, false]
        );
    }

    #[test]
    fn a_program_with_a_slash_is_checked_as_is() {
        let dir = bin();
        let codex = dir.path().join("codex").display().to_string();
        assert_eq!([found(&codex, None), found("/nonexistent/codex", None)], [true, false]);
    }

    #[test]
    fn without_a_path_nothing_is_found() {
        assert!(!found("sh", None));
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn config() -> Config {
    Config::default()
}

mod modes {
    use super::*;

    #[test]
    fn claude_has_the_default_modes() {
        let names: Vec<String> = modes(&config(), "claude").into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["accept edits", "auto", "plan", "skip permissions (dangerous)"]);
    }

    #[rstest]
    #[case::short(&["gemini", "-y"], "yolo (dangerous)")]
    #[case::long(&["gemini", "--yolo"], "yolo (dangerous)")]
    #[case::approval(&["gemini", "--approval-mode", "yolo"], "yolo (dangerous)")]
    #[case::equals(&["gemini", "--approval-mode=yolo"], "yolo (dangerous)")]
    #[case::plan(&["gemini", "--approval-mode", "plan"], "plan")]
    fn gemini_modes_include_plan_and_recognize_yolo_aliases(#[case] args: &[&str], #[case] expected: &str) {
        let modes = modes(&config(), Known::Gemini.name());
        assert_eq!(mode_of(Known::Gemini.name(), &strings(args), &modes).as_deref(), Some(expected));
        assert_eq!(
            with_mode(Known::Gemini.name(), &strings(&args[1..]), &modes, Some("plan")),
            strings(&["--approval-mode", "plan"])
        );
    }

    #[test]
    fn opencode_plans_with_its_plan_agent_and_approves_with_auto() {
        let kind = Known::Opencode.name();
        let modes = modes(&config(), kind);
        let names: Vec<&str> = modes.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["plan", "auto (dangerous)"]);
        assert_eq!(mode_of(kind, &strings(&["opencode", "--agent", "plan"]), &modes).as_deref(), Some("plan"));
        assert_eq!(
            with_mode(kind, &strings(&["--auto", "-c"]), &modes, Some("plan")),
            strings(&["--agent", "plan", "-c"])
        );
    }

    #[test]
    fn grok_modes_follow_its_permission_flags() {
        let modes = modes(&config(), Known::Grok.name());
        let kind = Known::Grok.name();
        assert_eq!(mode_of(kind, &strings(&["grok", "--permission-mode", "auto"]), &modes).as_deref(), Some("auto"));
        assert_eq!(mode_of(kind, &strings(&["grok", "--yolo"]), &modes).as_deref(), Some("always approve (dangerous)"));
        assert_eq!(mode_of(kind, &strings(&["grok", "--permission-mode=dontAsk"]), &modes), None);
    }

    #[rstest]
    #[case::qwen("qwen")]
    #[case::custom("mine")]
    fn another_agents_yolo_mode_keeps_its_own_arguments(#[case] kind: &str) {
        let mut config = config();
        config.agent_modes.insert(kind.into(), [("yolo".into(), strings(&["--yolo"]))].into());
        let modes = modes(&config, kind);
        let args = strings(&["-y", "--approval-mode=custom"]);
        assert_eq!(mode_of(kind, &args, &modes), None);
        assert_eq!(extra_args(kind, &args, &modes), args);
        assert_eq!(with_mode(kind, &args, &modes, None), args);
        assert_eq!(with_mode(kind, &args, &modes, Some("yolo")), strings(&["--yolo", "-y", "--approval-mode=custom"]));
    }

    #[test]
    fn the_config_adds_replaces_and_hides_modes() {
        let mut c = config();
        let mine = BTreeMap::from([
            ("plan".to_string(), strings(&["--plan"])),
            ("auto".to_string(), Vec::new()),
            ("quiet".to_string(), strings(&["-q"])),
        ]);
        c.agent_modes.insert("claude".into(), mine);
        let found = modes(&c, "claude");
        assert_eq!(
            found.iter().map(|(n, a)| (n.as_str(), a.join(" "))).collect::<Vec<_>>(),
            [
                ("accept edits", "--permission-mode acceptEdits".to_string()),
                ("plan", "--plan".into()),
                ("skip permissions (dangerous)", "--dangerously-skip-permissions".into()),
                ("quiet", "-q".into())
            ]
        );
    }

    #[test]
    fn a_mode_is_found_in_the_arguments() {
        let args = strings(&["--add-dir", "x", "--permission-mode", "plan"]);
        assert_eq!(mode_of(Known::Claude.name(), &args, &modes(&config(), "claude")).as_deref(), Some("plan"));
    }

    #[test]
    fn extra_arguments_are_the_rest() {
        let args = strings(&["--add-dir", "x", "--permission-mode", "plan"]);
        assert_eq!(extra_args(Known::Claude.name(), &args, &modes(&config(), "claude")), strings(&["--add-dir", "x"]));
    }

    #[test]
    fn picking_a_mode_replaces_the_old_one_and_keeps_the_extras() {
        let args = strings(&["--permission-mode", "plan", "--add-dir", "x"]);
        let next =
            with_mode(Known::Claude.name(), &args, &modes(&config(), "claude"), Some("skip permissions (dangerous)"));
        assert_eq!(next, strings(&["--dangerously-skip-permissions", "--add-dir", "x"]));
    }

    #[test]
    fn no_mode_keeps_only_the_extras() {
        let args = strings(&["--permission-mode", "plan", "-v"]);
        assert_eq!(with_mode(Known::Claude.name(), &args, &modes(&config(), "claude"), None), strings(&["-v"]));
    }

    #[test]
    fn new_extras_keep_the_mode() {
        let args = strings(&["--sandbox", "read-only", "-v"]);
        let next = with_extra(Known::Codex.name(), &args, &modes(&config(), "codex"), strings(&["--model", "o3"]));
        assert_eq!(next, strings(&["--sandbox", "read-only", "--model", "o3"]));
    }

    #[rstest]
    #[case::dangerous("skip permissions (dangerous)", true)]
    #[case::safe("plan", false)]
    fn dangerous_modes_say_so_in_their_name(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(is_dangerous(name), expected);
    }
}

mod arguments {
    use super::*;

    #[rstest]
    #[case::plain("--add-dir x -v", &["--add-dir", "x", "-v"])]
    #[case::single_quotes("--add-dir '../my dir'", &["--add-dir", "../my dir"])]
    #[case::double_quotes_escape(r#"-m "say \"hi\"""#, &["-m", r#"say "hi""#])]
    #[case::backslash_space(r"a\ b", &["a b"])]
    #[case::empty_quotes("''", &[""])]
    #[case::nothing("   ", &[])]
    fn split_like_a_shell(#[case] text: &str, #[case] expected: &[&str]) {
        assert_eq!(split_args(text), strings(expected));
    }

    #[test]
    fn cursor_modes_follow_its_flags() {
        let modes = modes(&config(), Known::Cursor.name());
        let kind = Known::Cursor.name();
        let mode = |argv: &[&str]| mode_of(kind, &strings(argv), &modes);
        assert_eq!(mode(&["cursor-agent", "--plan"]).as_deref(), Some("plan"));
        assert_eq!(mode(&["cursor-agent", "--mode=ask"]).as_deref(), Some("ask"));
        assert_eq!(mode(&["cursor-agent", "--yolo"]).as_deref(), Some("run everything (dangerous)"));
        assert_eq!(mode(&["cursor-agent", "-f", "--trust"]).as_deref(), Some("run everything (dangerous)"));
        assert_eq!(mode(&["cursor-agent", "--mode", "debug"]), None);
    }

    #[test]
    fn copilot_modes_follow_its_flags() {
        let modes = modes(&config(), Known::Copilot.name());
        let kind = Known::Copilot.name();
        let mode = |argv: &[&str]| mode_of(kind, &strings(argv), &modes);
        assert_eq!(mode(&["copilot", "--plan"]).as_deref(), Some("plan"));
        assert_eq!(mode(&["copilot", "--mode=autopilot"]).as_deref(), Some("autopilot"));
        assert_eq!(mode(&["copilot", "--autopilot"]).as_deref(), Some("autopilot"));
        assert_eq!(mode(&["copilot", "--yolo"]).as_deref(), Some("allow all (dangerous)"));
        assert_eq!(mode(&["copilot", "--allow-all", "--no-ask-user"]).as_deref(), Some("allow all (dangerous)"));
        assert_eq!(mode(&["copilot", "--mode", "debug"]), None);
    }

    #[test]
    fn vibe_modes_follow_its_agent_flag() {
        let modes = modes(&config(), Known::Vibe.name());
        let kind = Known::Vibe.name();
        let mode = |argv: &[&str]| mode_of(kind, &strings(argv), &modes);
        assert_eq!(mode(&["vibe", "--agent", "ask"]).as_deref(), Some("ask"));
        assert_eq!(mode(&["vibe", "--agent=smart-approve"]).as_deref(), Some("smart approve"));
        assert_eq!(mode(&["vibe", "--smart-approve"]).as_deref(), Some("smart approve"));
        assert_eq!(mode(&["vibe", "--yolo"]).as_deref(), Some("auto-approve (dangerous)"));
        assert_eq!(mode(&["vibe", "--auto-approve"]).as_deref(), Some("auto-approve (dangerous)"));
        assert_eq!(mode(&["vibe", "--agent", "debug"]), None);
    }

    #[test]
    fn join_quotes_what_a_shell_would_split() {
        assert_eq!(join_args(&strings(&["--add-dir", "../my dir", "it's"])), r"--add-dir '../my dir' 'it'\''s'");
    }

    #[test]
    fn the_command_line_is_the_binary_and_its_arguments() {
        let mut c = config();
        c.agent_args.insert("cursor".into(), strings(&["--model", "a b"]));
        assert_eq!(command_line(&c, "cursor"), "cursor-agent --model 'a b'");
    }

    #[rstest]
    #[case::added("claude", &["-v"], &["-v", "--model", "opus"])]
    #[case::replaced("claude", &["--model", "haiku", "-v"], &["-v", "--model", "opus"])]
    #[case::replaced_with_equals("claude", &["--model=haiku", "-v"], &["-v", "--model", "opus"])]
    #[case::claude_has_no_short_flag("claude", &["-m", "x"], &["-m", "x", "--model", "opus"])]
    #[case::short("codex", &["-m", "o3", "--search"], &["--search", "--model", "opus"])]
    #[case::short_with_equals("gemini", &["-m=pro", "-y"], &["-y", "--model", "opus"])]
    #[case::every_spelling("grok", &["-m", "a", "--model", "b", "--model=c", "-m=d", "-v"], &["-v", "--model", "opus"])]
    fn a_model_replaces_every_spelling_of_the_one_in_the_arguments(
        #[case] kind: &str,
        #[case] args: &[&str],
        #[case] expected: &[&str],
    ) {
        assert_eq!(with_model(kind, strings(args), Some("opus")), strings(expected));
        assert_eq!(with_model(kind, strings(args), Some(" ")), strings(args));
        assert_eq!(with_model("aider", strings(args), Some("opus")), strings(args));
    }

    #[rstest]
    #[case::claude("claude", &["--effort", "low", "-v", "--effort=max"], &["-v", "--effort", "high"])]
    #[case::grok_and_its_alias("grok", &["--reasoning-effort", "low", "--effort=max", "-v"], &["-v", "--reasoning-effort", "high"])]
    #[case::codex_config(
        "codex",
        &["-c", "model_reasoning_effort=low", "-c", "model=o3", "--config=model_reasoning_effort=low", "-cmodel_reasoning_effort=x"],
        &["-c", "model=o3", "-c", "model_reasoning_effort=high"]
    )]
    #[case::codex_long_config("codex", &["--config", "model_reasoning_effort=low"], &["-c", "model_reasoning_effort=high"])]
    #[case::gemini_has_none("gemini", &["--effort", "low"], &["--effort", "low"])]
    fn an_effort_replaces_every_spelling_of_the_one_in_the_arguments(
        #[case] kind: &str,
        #[case] args: &[&str],
        #[case] expected: &[&str],
    ) {
        assert_eq!(with_effort(kind, strings(args), Some("high")), strings(expected));
        assert_eq!(with_effort(kind, strings(args), None), strings(args));
    }

    #[test]
    fn a_start_takes_an_effort_the_agent_knows() {
        let c = config();
        assert_eq!(start_line(&c, "claude", None, None, Some("high")).as_deref(), Ok("claude --effort high"));
        assert_eq!(
            start_line(&c, "codex", None, None, Some("high")).as_deref(),
            Ok("codex -c model_reasoning_effort=high")
        );
        assert_eq!(
            start_line(&c, "claude", None, None, Some("huge")),
            Err("claude has no effort `huge`; its levels are `low`, `medium`, `high`, `xhigh`, `max`".into())
        );
        assert_eq!(
            start_line(&c, "copilot", None, None, Some("high")).as_deref(),
            Ok("copilot --reasoning-effort high")
        );
        assert_eq!(
            start_line(&c, "copilot", None, None, Some("turbo")),
            Err("copilot has no effort `turbo`; its levels are `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`".into())
        );
        assert_eq!(start_line(&c, "gemini", None, None, Some("high")), Err("gemini has no effort switch".into()));
    }

    #[test]
    fn a_start_picks_a_mode_and_a_model_over_the_settings() {
        let mut c = config();
        c.agent_args.insert("claude".into(), strings(&["--permission-mode", "auto", "--model", "haiku", "-v"]));
        assert_eq!(
            start_line(&c, "claude", None, None, None).as_deref(),
            Ok("claude --permission-mode auto --model haiku -v")
        );
        assert_eq!(
            start_line(&c, "claude", Some("plan"), Some("opus"), None).as_deref(),
            Ok("claude --permission-mode plan -v --model opus")
        );
    }

    #[test]
    fn a_start_with_a_mode_or_model_it_does_not_know_names_what_it_knows() {
        let c = config();
        assert_eq!(
            start_line(&c, "codex", Some("yolo"), None, None),
            Err("codex has no mode `yolo`; its modes are `read only`, `workspace write`, `no sandbox, no approvals \
                 (dangerous)`"
                .into())
        );
        assert_eq!(
            start_line(&c, "aider", Some("plan"), None, None),
            Err("aider has no modes in cornercase; add some with agent_modes in config.json".into())
        );
        assert_eq!(
            start_line(&c, "vibe", None, Some("mistral-medium-latest"), None),
            Err("vibe has no model flag; set its model in its own configuration".into())
        );
        let refused = start_line(&c, "aider", None, Some("gpt"), None).expect_err("aider takes no model");
        assert_eq!(
            refused,
            "cornercase passes a model to claude, codex, gemini, opencode, grok, cursor, copilot only; give aider its model in agent_args"
        );
    }

    #[test]
    fn agent_commands_rename_a_binary_and_add_kinds() {
        let mut c = config();
        c.agent_commands.insert("mine".into(), "/opt/bin/my-agent".into());
        assert_eq!((command(&c, "mine"), kinds(&c).contains(&"mine".to_string())), ("/opt/bin/my-agent".into(), true));
    }
}

mod resuming {
    use super::*;

    fn agent(kind: &str, mode: Option<&str>) -> AgentState {
        AgentState { kind: kind.into(), conversation: "4f2c-91".into(), mode: mode.map(str::to_string), account: None }
    }

    #[rstest]
    #[case::claude("claude", None, "claude --resume 4f2c-91")]
    #[case::claude_in_its_mode("claude", Some("plan"), "claude --permission-mode plan --resume 4f2c-91")]
    #[case::codex("codex", None, "codex resume 4f2c-91")]
    #[case::gemini(Known::Gemini.name(), Some("plan"), "gemini --approval-mode plan --resume 4f2c-91")]
    #[case::grok(Known::Grok.name(), Some("accept edits"), "grok --permission-mode acceptEdits --resume 4f2c-91")]
    #[case::grok_always(Known::Grok.name(), Some("always approve (dangerous)"), "grok --always-approve --resume 4f2c-91")]
    #[case::codex_in_its_mode("codex", Some("read only"), "codex resume --sandbox read-only 4f2c-91")]
    #[case::cursor_in_plan(Known::Cursor.name(), Some("plan"), "cursor-agent --mode plan --resume 4f2c-91")]
    #[case::cursor_running_everything(
        Known::Cursor.name(),
        Some("run everything (dangerous)"),
        "cursor-agent --force --resume 4f2c-91"
    )]
    #[case::copilot_in_plan(Known::Copilot.name(), Some("plan"), "copilot --mode plan --resume=4f2c-91")]
    #[case::copilot_allowing_all(
        Known::Copilot.name(),
        Some("allow all (dangerous)"),
        "copilot --allow-all --resume=4f2c-91"
    )]
    fn the_conversation_comes_back_in_the_mode_it_ran_in(
        #[case] kind: &str,
        #[case] mode: Option<&str>,
        #[case] expected: &str,
    ) {
        assert_eq!(resume_line(&config(), &agent(kind, mode)).as_deref(), Some(expected));
    }

    #[test]
    fn the_extra_arguments_stay_and_the_configured_mode_gives_way() {
        let mut c = config();
        c.agent_args.insert("claude".into(), strings(&["--permission-mode", "auto", "--add-dir", "../my dir"]));
        c.agent_commands.insert("claude".into(), "/opt/claude".into());
        assert_eq!(
            resume_line(&c, &agent("claude", None)).as_deref(),
            Some("/opt/claude --add-dir '../my dir' --resume 4f2c-91")
        );
    }

    #[test]
    fn other_agents_have_no_way_to_resume() {
        assert_eq!(resume_line(&config(), &agent("aider", None)), None);
    }
}

mod choosing {
    use super::*;

    #[rstest]
    #[case::explicit_wins(Some("codex"), "claude", Some("gemini"), Some("codex"))]
    #[case::then_the_config(None, "claude", Some("gemini"), Some("claude"))]
    #[case::auto_uses_the_running_one(None, "auto", Some("gemini"), Some("gemini"))]
    #[case::otherwise_nothing(None, "auto", None, None)]
    fn follows_the_resolution_order(
        #[case] explicit: Option<&str>,
        #[case] default: &str,
        #[case] running: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        let c = Config { agent: default.into(), ..config() };
        assert_eq!(resolve(&c, explicit, running).as_deref(), expected);
    }

    #[rstest]
    #[case::binary(&["/home/a/.local/bin/claude", "--resume"], Some("claude"))]
    #[case::node_script(&["node", "/usr/lib/node_modules/@google/gemini-cli/bin/gemini"], Some("gemini"))]
    #[case::codex_npm(&["node", "/usr/lib/node_modules/@openai/codex/bin/codex.js"], Some("codex"))]
    #[case::codex_native(&["/usr/local/bin/codex"], Some("codex"))]
    #[case::renamed_binary(&["cursor-agent"], Some("cursor"))]
    #[case::cursor_through_its_launcher(
        &["/home/a/.local/bin/agent", "--use-system-ca", "/home/a/.local/share/cursor-agent/versions/2026.10.14-941492e/index.js"],
        Some("cursor")
    )]
    #[case::cursor_through_its_shim(
        &["/home/a/.local/bin/agent", "--use-system-ca", "/home/a/.local/share/cursor-agent/versions/2026.10.14-941492e/index.js", "agent"],
        Some("cursor")
    )]
    #[case::cursor_by_name(&["/home/a/.local/bin/cursor-agent", "--use-system-ca", "/x/index.js"], Some("cursor"))]
    #[case::copilot_npm(&["node", "/usr/lib/node_modules/@github/copilot/npm-loader.js"], Some("copilot"))]
    #[case::copilot_npx(&["node", "/home/a/.npm/_npx/9f2/node_modules/@github/copilot/npm-loader.js"], Some("copilot"))]
    #[case::copilot_standalone(&["/home/a/.local/bin/copilot"], Some("copilot"))]
    #[case::gh_copilot(&["gh", "copilot", "--", "--model", "auto"], Some("copilot"))]
    #[case::vibe_through_its_launcher(
        &["/home/a/.local/share/uv/tools/mistral-vibe/lib/python3.14/site-packages/vibe/_bin/vibe-rs"],
        Some("vibe")
    )]
    #[case::vibe_after_flags(&["python3", "-I", "-B", "-u", "/x/vibe/_bin/vibe-rs"], Some("vibe"))]
    #[case::vibe_as_a_later_argument(&["less", "notes.txt", "/x/vibe/_bin/vibe-rs"], None)]
    #[case::another_npm_loader(&["node", "/x/node_modules/other/npm-loader.js"], None)]
    #[case::grok_binary_named_agent(&["/home/a/.grok/bin/agent"], None)]
    #[case::a_worker_server_of_another_app(&["node", "/home/a/app/versions/1/index.js", "worker-server"], None)]
    #[case::a_shell(&["-zsh"], None)]
    fn detects_the_agent_in_a_tab(#[case] argv: &[&str], #[case] expected: Option<&str>) {
        assert_eq!(detect(&config(), &strings(argv)).as_deref(), expected);
    }
}

mod trust {
    use super::*;

    fn asks(pattern: &str, screen: &str) -> bool {
        TrustPrompt::default().regex(pattern).is_some_and(|re| asks_trust(re, screen))
    }

    #[test]
    fn recognises_claudes_folder_question() {
        assert!(asks(DEFAULT_TRUST_PROMPT, "Do you trust the files in this folder?\n❯ 1. Yes, proceed"));
    }

    #[test]
    fn ignores_other_screens() {
        assert!(!asks(DEFAULT_TRUST_PROMPT, "> How can I help?"));
    }

    #[test]
    fn looks_only_at_the_last_lines_of_the_screen() {
        let screen = format!("$ cat notes\ndo you trust me?\n{}> How can I help?", "line\n".repeat(TRUST_LINES));
        assert!(!asks(DEFAULT_TRUST_PROMPT, &screen));
    }

    #[test]
    fn blank_lines_do_not_push_the_question_out() {
        let screen = format!("Do you trust the files in this folder?{}❯ 1. Yes, proceed", "\n".repeat(40));
        assert!(asks(DEFAULT_TRUST_PROMPT, &screen));
    }

    #[test]
    fn an_invalid_pattern_never_matches() {
        assert!(!asks("trust(", "Do you trust the files?"));
    }

    #[test]
    fn follows_a_changed_pattern() {
        let mut trust = TrustPrompt::default();
        trust.regex(DEFAULT_TRUST_PROMPT);
        assert!(trust.regex("how can i help").is_some_and(|re| asks_trust(re, "> How can I help?")));
    }
}
