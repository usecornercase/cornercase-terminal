use std::collections::BTreeMap;

use rstest::rstest;

use super::*;

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn for_powershell(line: &str, runs: impl Fn(&str) -> Option<PathBuf>) -> Result<String, String> {
    typed_on_windows(line, PaneShell::PowerShell, runs).map_err(|refusal| refusal.message)
}

const POWERSHELL_CASES: &[&[&str]] = &[
    &["claude", "--model", "a\u{2019}; Write-Output INJECTED; \u{2019}"],
    &["claude", "--model", "a\u{2018}; Write-Output INJECTED; \u{2018}"],
    &["claude", "--model", "a\u{201A}; Write-Output INJECTED; \u{201B}"],
    &["claude", "--model", "it's", "'", "''", "Sergio\u{2019}s code"],
    &["claude", "--%", "a | Write-Output INJECTED"],
    &["claude", "@notes", "$env:PATH", "`n", "(1)", "{1}", "<x", ">x", "#x", "a,b", "a;b", "a&b", "a|b", "--", "%x"],
    &["claude", r"--add-dir=C:\Users\me\R&D", "-x:y", "opus[1m]", "50%", r#"{"a":1}"#, ""],
    &["claude", "\u{2013}%", "\u{2014}x", "\u{201C}a\u{201D}", "1kb", "0x10", "-1", r".\x", "a=b", "+x"],
    &[r"C:\Program Files\Claude\claude.exe", "--resume", "a1"],
    &["-claude", "x"],
    &["/opt/claude", "x"],
    &[r"..\bin\claude.exe", "x"],
    &["claude", "--add-dir", r#"a b" --dangerously-skip-permissions "c"#, r"C:\my dir\", "a\tb\\", r"C:\dir\"],
    &["claude", "--model", "opus --some-flag", "--%", "--%x", "x--%", "--% "],
];

#[cfg(windows)]
mod powershell {
    use std::process::Command;

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    const PARSE: &str = r"
        foreach ($encoded in @(LINES)) {
            $line = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($encoded))
            $tokens = $null
            $errors = $null
            $ast = [System.Management.Automation.Language.Parser]::ParseInput($line, [ref]$tokens, [ref]$errors)
            $statements = $ast.EndBlock.Statements
            $command = $null
            if ($statements.Count -eq 1 -and $statements[0].PipelineElements.Count -eq 1) {
                $command = $statements[0].PipelineElements[0]
            }
            if ($errors.Count -ne 0 -or $command -isnot [System.Management.Automation.Language.CommandAst]) {
                'not one command'
                continue
            }
            $words = foreach ($element in $command.CommandElements) {
                $text = $element.Extent.Text
                if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
                    $text = $element.Value
                }
                [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($text))
            }
            $called = ''
            if ($command.InvocationOperator -eq 'Ampersand') { $called = '&' }
            $called + ($words -join '.')
        }
    ";

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    pub(super) fn parse(lines: &[String]) -> Vec<Option<(bool, Vec<String>)>> {
        let encoded: Vec<String> = lines.iter().map(|line| format!("'{}'", STANDARD.encode(utf16(line)))).collect();
        let script = PARSE.replace("LINES", &encoded.join(","));
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &STANDARD.encode(utf16(&script))])
            .output()
            .expect("run powershell");
        let parsed: Vec<_> = String::from_utf8_lossy(&output.stdout).lines().map(command).collect();
        assert_eq!(parsed.len(), lines.len(), "{output:?}");
        parsed
    }

    fn command(printed: &str) -> Option<(bool, Vec<String>)> {
        let (called, words) = printed.strip_prefix('&').map_or((false, printed), |words| (true, words));
        Some((called, words.split('.').map(decode).collect::<Option<Vec<String>>>()?))
    }

    fn decode(word: &str) -> Option<String> {
        let bytes = STANDARD.decode(word).ok()?;
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        String::from_utf16(&units).ok()
    }
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
    fn a_typed_line_quotes_for_the_shell_of_this_system() {
        let line = typed_line(strings(&["claude", "--model", "opus[1m]", "it's"]));
        let expected =
            if cfg!(windows) { "claude --model 'opus[1m]' 'it''s'" } else { r"claude --model 'opus[1m]' 'it'\''s'" };
        assert_eq!(line, expected);
    }

    #[rstest]
    #[case::a_flag("--permission-mode=plan", "--permission-mode=plan")]
    #[case::a_relative_folder(r"..\docs", r"..\docs")]
    #[case::a_folder_with_its_drive(r"C:\Users\me\repo", r"'C:\Users\me\repo'")]
    #[case::a_folder_with_a_space(r"C:\Users\me\my repo", r"'C:\Users\me\my repo'")]
    #[case::an_apostrophe("it's", "'it''s'")]
    #[case::a_typographic_apostrophe("Sergio\u{2019}s code", "'Sergio\u{2019}\u{2019}s code'")]
    #[case::every_single_quote_powershell_reads(
        "'\u{2018}\u{2019}\u{201A}\u{201B}",
        "'''\u{2018}\u{2018}\u{2019}\u{2019}\u{201A}\u{201A}\u{201B}\u{201B}'"
    )]
    #[case::stop_parsing("--%", "'--%'")]
    #[case::end_of_parameters("--", "'--'")]
    #[case::a_splat("@notes", "'@notes'")]
    #[case::a_variable("$env:PATH", "'$env:PATH'")]
    #[case::an_escape("`n", "'`n'")]
    #[case::a_list("Read,Write", "'Read,Write'")]
    #[case::a_comment("#x", "'#x'")]
    #[case::a_sign_first("+x", "'+x'")]
    #[case::a_percent("50%", "'50%'")]
    #[case::a_typographic_dash("\u{2013}%", "'\u{2013}%'")]
    #[case::empty("", "''")]
    fn a_word_is_quoted_for_powershell_unless_every_character_is_plain(#[case] word: &str, #[case] expected: &str) {
        assert_eq!(powershell_line(["claude".to_string(), word.to_string()]), format!("claude {expected}"));
    }

    #[rstest]
    #[case::with_a_space(&[r"C:\Program Files\Claude\claude.exe", "--resume", "a1"], r"& 'C:\Program Files\Claude\claude.exe' --resume a1")]
    #[case::starting_with_a_dash(&["-claude", "x"], "& '-claude' x")]
    #[case::by_its_name(&["claude", "x"], "claude x")]
    #[case::by_a_relative_path(&[r".\claude.exe", "x"], r".\claude.exe x")]
    #[case::by_a_unix_path(&["/opt/claude", "x"], "/opt/claude x")]
    fn powershell_calls_a_quoted_program_with_its_call_operator(#[case] words: &[&str], #[case] expected: &str) {
        assert_eq!(powershell_line(strings(words)), expected);
    }

    #[cfg(windows)]
    #[test]
    fn powershell_parses_each_typed_line_as_one_command_with_the_words_given() {
        let cases: Vec<Vec<String>> = POWERSHELL_CASES.iter().map(|words| strings(words)).collect();
        let lines: Vec<String> = cases.iter().map(|words| powershell_line(words.clone())).collect();
        let parsed = powershell::parse(&lines);
        for ((words, line), found) in cases.iter().zip(&lines).zip(parsed) {
            assert_eq!(found, Some((line.starts_with("& "), words.clone())), "{line}");
        }
    }

    #[test]
    fn each_typed_line_reads_back_as_the_words_powershell_parses() {
        for words in POWERSHELL_CASES.iter().map(|words| strings(words)) {
            let line = powershell_line(words.clone());
            assert_eq!(typed_words(&line), Some(words), "{line}");
        }
    }

    #[rstest]
    #[case::a_double_quote_with_a_space(r#"a b" --dangerously-skip-permissions "c"#, "`\"`")]
    #[case::json(r#"{"a":1}"#, "`\"`")]
    #[case::a_folder_ending_with_its_separator(r"C:\my dir\", "ends with")]
    #[case::a_tab_and_backslashes("a\tb\\\\", "ends with")]
    fn a_word_powershell_would_pass_on_in_pieces_is_refused_for_any_program(#[case] word: &str, #[case] why: &str) {
        let line = powershell_line(strings(&["claude", "--add-dir", word, "-v"]));
        let refused = for_powershell(&line, |_| Some(PathBuf::from(r"C:\bin\claude.exe"))).expect_err("refused");
        assert!(refused.contains(word) && refused.contains(why), "{refused}");
    }

    #[rstest]
    #[case::a_folder_ending_with_its_separator(r"C:\dir\")]
    #[case::a_backslash_inside(r"C:\my dir\x")]
    #[case::typographic_double_quotes("\u{201C}a b\u{201D}")]
    fn a_word_powershell_passes_on_whole_is_typed(#[case] word: &str) {
        let line = powershell_line(strings(&["claude", "--add-dir", word]));
        assert_eq!(for_powershell(&line, |_| Some(PathBuf::from(r"C:\bin\claude.exe"))), Ok(line));
    }

    #[rstest]
    #[case::alone(&["claude", "--%", "opus --some-flag"], true)]
    #[case::last(&["claude", "--model", "opus", "--%"], true)]
    #[case::inside_a_word(&["claude", "--%x", "x--%", "--% "], false)]
    fn the_stop_parsing_token_is_refused_as_a_word_of_its_own(#[case] words: &[&str], #[case] refused: bool) {
        let line = powershell_line(strings(words));
        let typed = typed_on_windows(&line, PaneShell::PowerShell, |_| Some(PathBuf::from(r"C:\bin\claude.exe")));
        let expected = if refused { Err("the stop-parsing token") } else { Ok(line) };
        assert_eq!(typed.map_err(|refusal| refusal.reason), expected);
    }

    #[rstest]
    #[case::json_in_powershell(PaneShell::PowerShell, r#"{"a":1}"#, Some("a word PowerShell would split"))]
    #[case::json_in_git_bash(PaneShell::Other, r#"{"a":1}"#, None)]
    #[case::stop_parsing_in_git_bash(PaneShell::Other, "--%", None)]
    #[case::an_ampersand_in_cmd(PaneShell::Cmd, r"C:\R&D", Some("a character cmd.exe reads"))]
    #[case::an_ampersand_in_powershell(PaneShell::PowerShell, r"C:\R&D", None)]
    #[case::an_ampersand_in_git_bash(PaneShell::Other, r"C:\R&D", None)]
    #[case::a_space_in_cmd(PaneShell::Cmd, "x --dangerously-skip-permissions y", Some("a word cmd.exe would split"))]
    #[case::a_space_in_powershell(PaneShell::PowerShell, "x --dangerously-skip-permissions y", None)]
    #[case::a_plain_word_in_cmd(PaneShell::Cmd, "opus", None)]
    fn what_is_refused_follows_the_panes_shell(
        #[case] shell: PaneShell,
        #[case] word: &str,
        #[case] reason: Option<&str>,
    ) {
        let line = powershell_line(strings(&["claude", "--add-dir", word]));
        let typed = typed_on_windows(&line, shell, |_| Some(PathBuf::from(r"C:\bin\claude.exe")));
        assert_eq!(typed.err().map(|refusal| refusal.reason), reason);
    }

    #[rstest]
    #[case::in_powershell(PaneShell::PowerShell)]
    #[case::in_cmd(PaneShell::Cmd)]
    fn powershell_and_cmd_panes_get_the_line_as_it_was_quoted(#[case] shell: PaneShell) {
        let line = powershell_line(strings(&["claude", "--model", "it's"]));
        let typed = typed_on_windows(&line, shell, |_| Some(PathBuf::from(r"C:\bin\claude.exe")));
        assert_eq!(typed.map_err(|refusal| refusal.reason), Ok(line));
    }

    #[rstest]
    #[case::a_plain_word("opus", "opus")]
    #[case::an_apostrophe("O'Brien", r"'O'\''Brien'")]
    #[case::a_typographic_apostrophe("Sergio\u{2019}s code", "'Sergio\u{2019}s code'")]
    #[case::every_single_quote_powershell_reads(
        "'\u{2018}\u{2019}\u{201A}\u{201B}",
        "''\\''\u{2018}\u{2019}\u{201A}\u{201B}'"
    )]
    #[case::json(r#"{"a":1}"#, r#"'{"a":1}'"#)]
    #[case::stop_parsing("--%", "--%")]
    #[case::an_ampersand(r"C:\R&D", r"'C:\R&D'")]
    #[case::a_relative_folder(r"..\docs", r"'..\docs'")]
    #[case::a_folder_ending_with_its_separator(r"C:\my dir\", r"'C:\my dir\'")]
    #[case::empty("", "''")]
    fn a_git_bash_pane_gets_each_word_quoted_for_a_posix_shell(#[case] word: &str, #[case] expected: &str) {
        let line = powershell_line(strings(&["claude", "--add-dir", word]));
        let typed = typed_on_windows(&line, PaneShell::Other, |_| Some(PathBuf::from(r"C:\bin\claude.exe")));
        assert_eq!(typed.map_err(|refusal| refusal.reason), Ok(format!("claude --add-dir {expected}")));
    }

    #[test]
    fn a_git_bash_pane_runs_a_quoted_program_without_powershells_call_operator() {
        let line = powershell_line(strings(&[r"C:\Program Files\Claude\claude.exe", "--resume", "a1"]));
        let typed = typed_on_windows(&line, PaneShell::Other, |program| Some(PathBuf::from(program)));
        assert_eq!(
            typed.map_err(|refusal| refusal.reason),
            Ok(r"'C:\Program Files\Claude\claude.exe' --resume a1".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_posix_shell_reads_each_git_bash_line_as_the_words_given() {
        for words in POWERSHELL_CASES.iter().map(|words| strings(words)) {
            let typed = typed_on_windows(&powershell_line(words.clone()), PaneShell::Other, |_| None);
            let line = typed.map_err(|refusal| refusal.reason).expect("typed");
            let script = format!("set -- {line}; printf '%s\\n' \"$@\"");
            let output = std::process::Command::new("/bin/sh").args(["-c", &script]).output().expect("run sh");
            let read: Vec<String> = String::from_utf8_lossy(&output.stdout).lines().map(str::to_string).collect();
            assert_eq!(read, words, "{line}");
        }
    }

    #[test]
    fn a_cmd_refusal_names_the_word_that_holds_the_character() {
        let line = powershell_line(strings(&["claude", "--add-dir", r"C:\R&D"]));
        let typed = typed_on_windows(&line, PaneShell::Cmd, |_| Some(PathBuf::from(r"C:\bin\claude.exe")));
        let message = typed.expect_err("a cmd pane refuses the `&`").message;
        assert!(message.contains(r"`&` in `C:\R&D`"), "{message}");
    }

    #[rstest]
    #[case::pwsh(Some("pwsh"), PaneShell::PowerShell)]
    #[case::windows_powershell(Some("powershell"), PaneShell::PowerShell)]
    #[case::cmd(Some("cmd"), PaneShell::Cmd)]
    #[case::git_bash(Some("bash"), PaneShell::Other)]
    #[case::unknown(None, PaneShell::PowerShell)]
    fn the_panes_shell_is_known_by_its_process_name(#[case] name: Option<&str>, #[case] expected: PaneShell) {
        assert_eq!(PaneShell::named(name), expected);
    }

    #[cfg(windows)]
    #[rstest]
    #[case::pwsh_exe("PWSH.EXE", PaneShell::PowerShell)]
    #[case::cmd_exe("Cmd.exe", PaneShell::Cmd)]
    fn on_windows_the_panes_shell_is_named_in_any_case_with_its_exe(#[case] name: &str, #[case] expected: PaneShell) {
        assert_eq!(PaneShell::named(Some(name)), expected);
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

mod batch_files {
    use super::*;
    use crate::test_util::{NPM_SCRIPT, NPM_SHIM, TempDir, npm_install, under};

    fn typed(words: &[&str], runs: &str) -> Result<String, String> {
        let runs = PathBuf::from(runs);
        for_powershell(&powershell_line(strings(words)), move |_| Some(runs.clone()))
    }

    fn refusal(words: &[&str], runs: &str) -> Option<String> {
        typed(words, runs).err()
    }

    fn shown(path: PathBuf) -> String {
        path.into_os_string().into_string().expect("utf-8")
    }

    fn through(shim: &Path, line: &str) -> Result<String, String> {
        for_powershell(line, |program| (program == "claude").then(|| shim.to_path_buf()))
    }

    const WORDS: [&str; 7] = ["--add-dir", r"C:\Users\me\R&D", "50%", "!x!", "a^b|c", "<in", "Sergio\u{2019}s code"];

    #[test]
    fn an_npm_shim_is_passed_over_only_in_a_powershell_pane() {
        let dir = TempDir::new();
        let shim = npm_install(dir.path(), NPM_SHIM, &[&NPM_SCRIPT, &["node.exe"]]);
        let runs = |program: &str| (program == "claude").then(|| shim.clone());
        assert_eq!(typed_on_windows("claude --resume a1", PaneShell::Other, runs), Ok("claude --resume a1".into()));
        let refused = typed_on_windows("claude 'R&D'", PaneShell::Other, runs).expect_err("refused");
        assert_eq!(refused.reason, "a batch file");
    }

    #[test]
    fn a_word_powershell_would_split_is_refused_past_an_npm_shim_too() {
        let dir = TempDir::new();
        let shim = npm_install(dir.path(), NPM_SHIM, &[&NPM_SCRIPT, &["node.exe"]]);
        let line = powershell_line(strings(&["claude", "--add-dir", r#"a b" --dangerously-skip-permissions "c"#]));
        let refused = through(&shim, &line).expect_err("refused");
        assert!(refused.contains("--dangerously-skip-permissions") && refused.contains("`\"`"), "{refused}");
    }

    #[test]
    fn an_npm_shim_is_passed_over_for_node_and_its_script_with_every_word() {
        let dir = TempDir::new();
        let shim = npm_install(dir.path(), NPM_SHIM, &[&NPM_SCRIPT, &["node.exe"]]);
        let node = [dir.path().join("node.exe"), under(dir.path(), &NPM_SCRIPT)].map(shown);
        let line = powershell_line(strings(&["claude"]).into_iter().chain(strings(&WORDS)));
        let expected = powershell_line(node.into_iter().chain(strings(&WORDS)));
        assert_eq!(through(&shim, &line), Ok(expected.clone()));
        let quoted = powershell_line(std::iter::once(shown(shim.clone())).chain(strings(&WORDS)));
        assert_eq!(for_powershell(&quoted, |program| Some(PathBuf::from(program))), Ok(expected));
    }

    #[test]
    fn an_npm_shim_alone_is_typed_with_no_word_after_it() {
        let dir = TempDir::new();
        let shim = npm_install(dir.path(), NPM_SHIM, &[&NPM_SCRIPT, &["node.exe"]]);
        let node = [dir.path().join("node.exe"), under(dir.path(), &NPM_SCRIPT)].map(shown);
        assert_eq!(through(&shim, "claude"), Ok(powershell_line(node)));
    }

    #[test]
    fn an_npm_shim_without_a_node_to_run_keeps_the_refusal() {
        let dir = TempDir::new();
        let shim = npm_install(dir.path(), NPM_SHIM, &[&NPM_SCRIPT]);
        let refused = through(&shim, r"claude --add-dir 'C:\Users\me\R&D'").expect_err("refused");
        assert!(refused.contains("`&`") && refused.contains("claude.cmd"), "{refused}");
    }

    #[cfg(windows)]
    #[test]
    fn powershell_reads_the_line_past_an_npm_shim_as_node_its_script_and_every_word_and_it_is_claude() {
        let dir = TempDir::new();
        let shim = npm_install(dir.path(), NPM_SHIM, &[&NPM_SCRIPT, &["node.exe"]]);
        let line = through(&shim, &powershell_line(strings(&["claude"]).into_iter().chain(strings(&WORDS))));
        let parsed = powershell::parse(&[line.expect("typed")]);
        let node = [dir.path().join("node.exe"), under(dir.path(), &NPM_SCRIPT)].map(shown);
        let argv: Vec<String> = node.into_iter().chain(strings(&WORDS)).collect();
        assert_eq!(parsed, [Some((true, argv.clone()))]);
        assert_eq!(detect(&config(), &argv).as_deref(), Some("claude"));
    }

    #[rstest]
    #[case::an_ampersand(r"C:\Users\me\R&D", '&')]
    #[case::a_pipe("a|b", '|')]
    #[case::a_redirection("a<b", '<')]
    #[case::another_redirection("a>b", '>')]
    #[case::cmds_escape("a^b", '^')]
    #[case::a_variable("%PATH%", '%')]
    #[case::a_delayed_variable("!x!", '!')]
    fn a_word_cmd_reads_is_refused_for_a_cmd_shim(#[case] word: &str, #[case] special: char) {
        let refused = refusal(&["claude", "--add-dir", word], r"C:\npm\claude.cmd").expect("refused");
        assert!(refused.contains(&format!("`{special}`")) && refused.contains(r"C:\npm\claude.cmd"), "{refused}");
    }

    #[rstest]
    #[case::the_ps1_shim(r"C:\npm\claude.ps1")]
    #[case::a_program(r"C:\Users\me\.local\bin\claude.exe")]
    fn the_same_word_is_quoted_and_typed_for_anything_else(#[case] runs: &str) {
        let words = ["claude", "--add-dir", r"C:\Users\me\R&D"];
        assert_eq!(powershell_line(strings(&words)), r"claude --add-dir 'C:\Users\me\R&D'");
        assert_eq!(typed(&words, runs), Ok(r"claude --add-dir 'C:\Users\me\R&D'".to_string()));
    }

    #[test]
    fn plain_words_go_to_a_cmd_shim_it_cannot_read() {
        let words = ["claude", "--permission-mode", "plan", r"..\docs"];
        assert_eq!(typed(&words, r"C:\npm\claude.CMD"), Ok(powershell_line(strings(&words))));
    }

    #[test]
    fn a_batch_file_named_as_the_program_counts_when_it_is_not_found() {
        let line = powershell_line(strings(&[r"C:\Program Files\x\claude.bat", "50%"]));
        let refused = for_powershell(&line, |_| None).expect_err("refused");
        assert!(refused.contains(r"`C:\Program Files\x\claude.bat`") && refused.contains("`%`"), "{refused}");
    }

    #[test]
    fn the_program_is_read_back_from_the_typed_line() {
        let line = powershell_line(strings(&[r"C:\R&D\Sergio’s\claude.cmd", "--resume", "a1"]));
        let refused = for_powershell(&line, |program| Some(PathBuf::from(program))).expect_err("refused");
        assert!(refused.contains(r"`C:\R&D\Sergio’s\claude.cmd`") && refused.contains("`&`"), "{refused}");
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

    #[cfg(windows)]
    #[rstest]
    #[case::native_binary(&[r"C:\Users\a\.local\bin\claude.exe", "--resume"], Some("claude"))]
    #[case::extension_in_capitals(&[r"C:\Users\a\.local\bin\claude.EXE"], Some("claude"))]
    #[case::name_in_capitals(&[r"C:\Users\a\.local\bin\CLAUDE.EXE"], Some("claude"))]
    #[case::binary_named_without_its_extension(&["claude"], Some("claude"))]
    #[case::copilot_npm_through_its_cmd_shim_which_doubles_a_separator(
        &[r"C:\Program Files\nodejs\node.exe", r"C:\Users\a\AppData\Roaming\npm\\node_modules\@github\copilot\npm-loader.js"],
        Some("copilot")
    )]
    #[case::copilot_npm_in_capitals(
        &[r"C:\Program Files\nodejs\node.exe", r"C:\Users\a\AppData\Roaming\npm\node_modules\@GitHub\Copilot\NPM-LOADER.JS"],
        Some("copilot")
    )]
    #[case::cursor_through_its_launcher_in_capitals(
        &[r"C:\Users\a\.local\bin\agent.exe", r"C:\Users\a\AppData\Local\Cursor-Agent\Versions\2026.10.14-941492e\INDEX.JS"],
        Some("cursor")
    )]
    #[case::claude_npm_script(
        &[r"C:\Program Files\nodejs\node.exe", r"C:\Users\a\AppData\Roaming\npm\\node_modules\@anthropic-ai\claude-code\cli.js"],
        Some("claude")
    )]
    #[case::claude_npm_binary(
        &[r"C:\Users\a\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe"],
        Some("claude")
    )]
    #[case::codex_npm_script(
        &[r"C:\Program Files\nodejs\node.exe", r"C:\Users\a\AppData\Roaming\npm\node_modules\@openai\codex\bin\codex.js"],
        Some("codex")
    )]
    #[case::codex_npm_script_in_capitals(
        &[r"C:\Program Files\nodejs\node.exe", r"C:\npm\node_modules\@openai\Codex\BIN\Codex.JS"],
        Some("codex")
    )]
    #[case::codex_native(&[r"C:\Users\a\AppData\Roaming\npm\node_modules\@openai\codex\vendor\codex.exe"], Some("codex"))]
    #[case::gemini_bundle(
        &[r"C:\Program Files\nodejs\node.exe", "--no-warnings=DEP0040", r"C:\npm\node_modules\@google\gemini-cli\bundle\gemini.js"],
        Some("gemini")
    )]
    #[case::gemini_dist(
        &[r"C:\Program Files\nodejs\node.exe", r"C:\npm\node_modules\@google\gemini-cli\dist\index.js"],
        Some("gemini")
    )]
    #[case::opencode_binary(&[r"C:\npm\node_modules\opencode-ai\bin\opencode.exe"], Some("opencode"))]
    #[case::opencode_launcher(&[r"C:\Program Files\nodejs\node.exe", r"C:\npm\node_modules\opencode-ai\bin\opencode"], Some("opencode"))]
    #[case::vibe_binary(&[r"C:\Users\a\AppData\Roaming\uv\tools\mistral-vibe\Lib\site-packages\vibe\_bin\Vibe-RS.EXE"], Some("vibe"))]
    #[case::another_cli_js(&[r"C:\Program Files\nodejs\node.exe", r"C:\npm\node_modules\other\cli.js"], None)]
    #[case::a_shell(&[r"C:\WINDOWS\system32\cmd.exe", "/d"], None)]
    fn detects_the_agent_in_a_windows_tab(#[case] argv: &[&str], #[case] expected: Option<&str>) {
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

#[cfg(windows)]
mod account_homes_on_windows {
    use super::*;

    const PROFILE: (&str, &str) = ("USERPROFILE", r"C:\Users\me");

    #[rstest]
    #[case::claude(Known::Claude, &[PROFILE], Some(r"C:\Users\me\.claude"))]
    #[case::codex(Known::Codex, &[PROFILE], Some(r"C:\Users\me\.codex"))]
    #[case::gemini(Known::Gemini, &[PROFILE], Some(r"C:\Users\me"))]
    #[case::home_is_not_read(Known::Claude, &[("HOME", r"C:\home"), PROFILE], Some(r"C:\Users\me\.claude"))]
    #[case::only_home(Known::Codex, &[("HOME", r"C:\home")], None)]
    #[case::the_variable_first(Known::Claude, &[PROFILE, ("CLAUDE_CONFIG_DIR", r"D:\claude")], Some(r"D:\claude"))]
    #[case::a_relative_profile(Known::Codex, &[("USERPROFILE", "me")], None)]
    fn are_in_the_profile_when_home_is_unset(
        #[case] kind: Known,
        #[case] env: &[(&str, &str)],
        #[case] expected: Option<&str>,
    ) {
        let var = |name: &str| env.iter().find(|(key, _)| *key == name).map(|(_, value)| OsString::from(*value));
        let found = kind.account().and_then(|account| account.default_home(var));
        assert_eq!(found, expected.map(PathBuf::from));
    }
}
