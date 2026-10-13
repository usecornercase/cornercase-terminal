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
        let modes = modes(&config(), GEMINI);
        assert_eq!(mode_of(GEMINI, &strings(args), &modes).as_deref(), Some(expected));
        assert_eq!(
            with_mode(GEMINI, &strings(&args[1..]), &modes, Some("plan")),
            strings(&["--approval-mode", "plan"])
        );
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
        assert_eq!(mode_of(CLAUDE, &args, &modes(&config(), "claude")).as_deref(), Some("plan"));
    }

    #[test]
    fn extra_arguments_are_the_rest() {
        let args = strings(&["--add-dir", "x", "--permission-mode", "plan"]);
        assert_eq!(extra_args(CLAUDE, &args, &modes(&config(), "claude")), strings(&["--add-dir", "x"]));
    }

    #[test]
    fn picking_a_mode_replaces_the_old_one_and_keeps_the_extras() {
        let args = strings(&["--permission-mode", "plan", "--add-dir", "x"]);
        let next = with_mode(CLAUDE, &args, &modes(&config(), "claude"), Some("skip permissions (dangerous)"));
        assert_eq!(next, strings(&["--dangerously-skip-permissions", "--add-dir", "x"]));
    }

    #[test]
    fn no_mode_keeps_only_the_extras() {
        let args = strings(&["--permission-mode", "plan", "-v"]);
        assert_eq!(with_mode(CLAUDE, &args, &modes(&config(), "claude"), None), strings(&["-v"]));
    }

    #[test]
    fn new_extras_keep_the_mode() {
        let args = strings(&["--sandbox", "read-only", "-v"]);
        let next = with_extra(CODEX, &args, &modes(&config(), "codex"), strings(&["--model", "o3"]));
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
    fn join_quotes_what_a_shell_would_split() {
        assert_eq!(join_args(&strings(&["--add-dir", "../my dir", "it's"])), r"--add-dir '../my dir' 'it'\''s'");
    }

    #[test]
    fn the_command_line_is_the_binary_and_its_arguments() {
        let mut c = config();
        c.agent_args.insert("cursor".into(), strings(&["--model", "a b"]));
        assert_eq!(command_line(&c, "cursor"), "cursor-agent --model 'a b'");
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
        AgentState { kind: kind.into(), conversation: "4f2c-91".into(), mode: mode.map(str::to_string) }
    }

    #[rstest]
    #[case::claude("claude", None, "claude --resume 4f2c-91")]
    #[case::claude_in_its_mode("claude", Some("plan"), "claude --permission-mode plan --resume 4f2c-91")]
    #[case::codex("codex", None, "codex resume 4f2c-91")]
    #[case::gemini(GEMINI, Some("plan"), "gemini --approval-mode plan --resume 4f2c-91")]
    #[case::codex_in_its_mode("codex", Some("read only"), "codex resume --sandbox read-only 4f2c-91")]
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

mod input_box {
    use rstest::rstest;

    use super::super::input_box;
    use super::super::{CLAUDE, CODEX};

    const RULE: &str = "────────────────────────────────────────";
    const FOOTER: &str = "  ⏵⏵ auto mode on · 1 shell · ← for agents";

    fn screen(lines: &[&str]) -> String {
        lines.join("\n")
    }

    #[rstest]
    #[case::empty(&["● Done.", "", RULE, "❯\u{a0}", RULE, FOOTER], Some(true))]
    #[case::with_text(&[RULE, "❯\u{a0}add tests", "  and run them", RULE, FOOTER], Some(true))]
    #[case::a_suggestion_list_above_it(&["  ❯ /model   Set the AI model", RULE, "❯\u{a0}/model", RULE], Some(true))]
    #[case::the_shell_details_panel(
        &["⎿  Command was manually backgrounded", RULE, "  Shell details", "  Status:   running",
          "  ← to go back · Esc/Enter/Space to close · x to stop"],
        Some(false)
    )]
    #[case::the_model_picker(&[RULE, "   Select model", "   ❯ 2.  Opus 5.5 ✔", "     3.  Fable 5.1"], Some(false))]
    #[case::the_transcript_view(&[RULE, "  Showing detailed transcript · ctrl+o to toggle"], Some(false))]
    #[case::the_prompt_history(&["  ❯ 49s ago  !sleep 300", "  ╭──────╮", "  │ ⌕ Filter history… │"], Some(false))]
    #[case::a_past_prompt_in_the_transcript(&["", "❯ fix the login", "", "● Fixed."], Some(false))]
    #[case::its_shell_mode(&[RULE, "! sleep 300", RULE, "  ! for shell mode"], Some(false))]
    fn claudes_box_is_its_marker_right_under_a_rule(#[case] lines: &[&str], #[case] expected: Option<bool>) {
        assert_eq!(input_box(CLAUDE, &screen(lines)), expected);
    }

    #[test]
    fn other_agents_are_not_known() {
        assert_eq!(input_box(CODEX, &screen(&["› Ask Codex to do anything"])), None);
    }
}

mod survey {
    use rstest::rstest;

    use super::super::survey;
    use super::super::{CLAUDE, CODEX};

    const RULE: &str = "────────────────────────────────────────";
    const QUESTION: &str = "● How is Claude doing this session? (optional)";
    const OPTIONS: &str = "  1: Bad    2: Fine   3: Good   0: Dismiss";
    const EFFORT: &str = "                                                  ◐ medium · /effort";
    const FOOTER: &str = "  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents";

    fn screen(lines: &[&str]) -> String {
        [lines, &[RULE, "❯\u{a0}", RULE, FOOTER]].concat().join("\n")
    }

    #[rstest]
    #[case::with_the_effort_hint_under_it(&["● Done.", "", QUESTION, OPTIONS, EFFORT])]
    #[case::with_a_blank_line_under_it(&[QUESTION, OPTIONS, ""])]
    #[case::its_question_wrapped(&["● How is Claude doing this session?", "  (optional)", OPTIONS, ""])]
    #[case::the_long_context_question(
        &["● How well is Claude following the instructions you gave earlier in this", "  conversation? (optional)",
          OPTIONS, ""]
    )]
    #[case::a_plugins_survey_with_unsure(
        &["● How helpful has the github plugin been? (optional)", "  You've used its skills recently",
          "  1: Bad    2: Fine   3: Good   4: Unsure 0: Dismiss", ""]
    )]
    fn is_its_question_and_answers_right_above_the_input_box(#[case] lines: &[&str]) {
        assert!(survey(CLAUDE, &screen(lines)));
    }

    #[rstest]
    #[case::the_input_box_alone(&["● Done.", ""])]
    #[case::the_words_in_the_conversation_above(
        &["❯ what does the survey look like", "",
          "● Claude Code shows a survey, How is Claude doing this session? (optional) with 1: Bad  2: Fine  3:",
          "  Good  0: Dismiss, above its input box. It looks like this:", "",
          "  ● How is Claude doing this session? (optional)", "    1: Bad    2: Fine   3: Good   0: Dismiss", "",
          "✻ Baked for 0s · done 6:18 PM", "", ""]
    )]
    #[case::a_quote_right_above_the_input_box(
        &["● It looks like this:", "", "  ● How is Claude doing this session? (optional)",
          "    1: Bad    2: Fine   3: Good   0: Dismiss", ""]
    )]
    #[case::the_issues_words_ending_a_message(
        &["● Here is how it reads:", "  How is Claude doing this session? (optional)",
          "  1: Bad  2: Fine  3: Good  0: Dismiss", ""]
    )]
    #[case::a_copy_ending_a_message_and_its_turn(
        &["● Here is how it reads:", "  How is Claude doing this session? (optional)", OPTIONS, "",
          "✻ Crunched for 0s · done 6:22 PM", ""]
    )]
    #[case::its_lines_higher_up(&[QUESTION, OPTIONS, "", "● Done.", "", "✻ Baked for 0s · done 6:18 PM", ""])]
    #[case::answers_without_their_question(&["● Done.", "", OPTIONS, ""])]
    #[case::answers_cut_in_a_very_narrow_pane(
        &["● How is Claude doing this session?", "  (optional)", "  1: Bad    2: Fine  3: Good   0:",
          "                               Dismiss", "                    ◐ medium · /effort"]
    )]
    fn is_nothing_else(#[case] lines: &[&str]) {
        assert!(!survey(CLAUDE, &screen(lines)));
    }

    #[test]
    fn needs_the_input_box_under_it() {
        let panel = [QUESTION, OPTIONS, "", RULE, "  Shell details", "  x to stop"].join("\n");
        assert!(!survey(CLAUDE, &panel));
    }

    #[test]
    fn is_only_claudes() {
        assert!(!survey(CODEX, &screen(&[QUESTION, OPTIONS, ""])));
    }
}
