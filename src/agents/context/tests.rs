use std::io::Write;

use rstest::rstest;

use super::*;
use crate::agents::activity::Session;
use crate::test_util::{Sleeper, TempDir};
use crate::time::Stamp;

fn assistant(model: &str, input: u64, created: u64, read: u64) -> String {
    format!(
        r#"{{"parentUuid":"p","isSidechain":false,"type":"assistant","message":{{"model":"{model}","role":"assistant","content":[{{"type":"text","text":"ok"}}],"usage":{{"input_tokens":{input},"cache_creation_input_tokens":{created},"cache_read_input_tokens":{read},"output_tokens":9}}}},"uuid":"u"}}"#
    )
}

fn user(text: &str) -> String {
    format!(r#"{{"isSidechain":false,"type":"user","message":{{"role":"user","content":"{text}"}},"uuid":"u"}}"#)
}

const COMPACTED: &str = r#"{"type":"system","subtype":"compact_boundary","content":"Conversation compacted","isMeta":false,"level":"info","compactMetadata":{"trigger":"manual","preTokens":180000}}"#;

fn reply(model: &str, tokens: u64) -> Reply {
    Reply { model: model.into(), tokens }
}

fn joined(lines: &[String]) -> String {
    lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|a| (*a).to_string()).collect()
}

fn write_file(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the folder");
    std::fs::write(path, text).expect("write the file");
}

struct Written {
    dir: TempDir,
    transcript: Transcript,
}

impl Written {
    fn new() -> Self {
        let dir = TempDir::new();
        let transcript = Transcript::new(dir.path().join("session.jsonl"));
        Self { dir, transcript }
    }

    fn append(&mut self, text: &str) -> Option<Reply> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.path().join("session.jsonl"))
            .expect("open the transcript");
        file.write_all(text.as_bytes()).expect("write the transcript");
        self.transcript.update();
        self.transcript.reply.clone()
    }

    fn lines(&mut self, lines: &[String]) -> Option<Reply> {
        self.append(&joined(lines))
    }
}

mod transcript {
    use super::*;

    #[test]
    fn the_last_reply_names_the_model_and_the_tokens_it_was_sent() {
        let mut file = Written::new();

        let found = file.lines(&[
            user("hi"),
            assistant("claude-sonnet-5-5", 1, 2, 3),
            assistant("claude-opus-5-5", 2, 15_655, 149_954),
        ]);

        assert_eq!(found, Some(reply("claude-opus-5-5", 165_611)));
    }

    #[test]
    fn lines_after_the_reply_keep_it() {
        let mut file = Written::new();

        let found = file.lines(&[assistant("claude-opus-5-5", 1, 0, 9), user("tool result")]);

        assert_eq!(found, Some(reply("claude-opus-5-5", 10)));
    }

    #[rstest]
    #[case::subagent(
        r#"{"isSidechain":true,"type":"assistant","message":{"model":"claude-haiku-4-5","usage":{"input_tokens":5}}}"#
    )]
    #[case::synthetic(r#"{"type":"assistant","message":{"model":"<synthetic>","usage":{"input_tokens":0}}}"#)]
    #[case::no_usage(r#"{"type":"assistant","message":{"model":"claude-opus-5-5"}}"#)]
    #[case::no_tokens(r#"{"type":"assistant","message":{"model":"claude-opus-5-5","usage":{"input_tokens":0}}}"#)]
    #[case::quoted_in_a_tool_result(
        r#"{"type":"user","message":{"content":"{\"type\":\"assistant\"}"},"toolUseResult":{"type":"assistant"}}"#
    )]
    #[case::broken(r#"{"type":"assistant","message":{"model":"claude-opus-5-5","usa"#)]
    fn some_lines_are_not_a_reply(#[case] line: &str) {
        let mut file = Written::new();

        let found = file.lines(&[assistant("claude-opus-5-5", 1, 0, 9), line.to_string()]);

        assert_eq!(found, Some(reply("claude-opus-5-5", 10)));
    }

    #[test]
    fn compacting_hides_it_until_the_next_reply() {
        let mut file = Written::new();
        file.lines(&[assistant("claude-opus-5-5", 1, 0, 180_000)]);

        let compacted = file.lines(&[COMPACTED.to_string(), user("summary")]);
        let answered = file.lines(&[assistant("claude-opus-5-5", 1, 0, 30_000)]);

        assert_eq!((compacted, answered), (None, Some(reply("claude-opus-5-5", 30_001))));
    }

    #[test]
    fn a_line_still_being_written_waits_for_its_end() {
        let mut file = Written::new();
        file.lines(&[assistant("claude-opus-5-5", 1, 0, 9)]);
        let line = assistant("claude-opus-5-5", 1, 0, 99);
        let (head, tail) = line.split_at(40);

        let half = file.append(head);
        let whole = file.append(&format!("{tail}\n"));

        assert_eq!((half, whole), (Some(reply("claude-opus-5-5", 10)), Some(reply("claude-opus-5-5", 100))));
    }

    #[test]
    fn only_the_last_mebibyte_is_read_at_first() {
        let mut file = Written::new();
        let filler = user(&"x".repeat(1000));
        let lines: Vec<String> =
            std::iter::once(assistant("claude-opus-4-1", 1, 0, 9)).chain(std::iter::repeat_n(filler, 1100)).collect();

        assert_eq!(file.lines(&lines), None);
    }

    #[test]
    fn a_reply_within_the_last_mebibyte_is_found() {
        let mut file = Written::new();
        let filler = user(&"x".repeat(1000));
        let lines: Vec<String> =
            std::iter::repeat_n(filler, 1100).chain(std::iter::once(assistant("claude-opus-5-5", 1, 0, 9))).collect();

        assert_eq!(file.lines(&lines), Some(reply("claude-opus-5-5", 10)));
    }

    #[test]
    fn a_compaction_in_a_large_append_is_not_skipped() {
        let mut file = Written::new();
        file.lines(&[assistant("claude-opus-5-5", 1, 0, 180_000)]);
        let lines: Vec<String> =
            std::iter::once(COMPACTED.to_string()).chain(std::iter::repeat_n(user(&"x".repeat(1000)), 1100)).collect();

        assert_eq!(file.lines(&lines), None);
    }

    #[test]
    fn a_file_that_starts_over_is_read_again() {
        let mut file = Written::new();
        file.lines(&[assistant("claude-opus-5-5", 1, 0, 9_999), user("a long question")]);
        std::fs::write(file.dir.path().join("session.jsonl"), format!("{}\n", assistant("claude-haiku-4-5", 1, 0, 9)))
            .expect("rewrite the transcript");

        file.transcript.update();

        assert_eq!(file.transcript.reply, Some(reply("claude-haiku-4-5", 10)));
    }

    #[test]
    fn a_transcript_that_goes_away_takes_its_reply_with_it() {
        let mut file = Written::new();
        file.lines(&[assistant("claude-opus-5-5", 1, 0, 9)]);
        std::fs::remove_file(file.dir.path().join("session.jsonl")).expect("remove the transcript");

        file.transcript.update();

        assert_eq!(file.transcript.reply, None);
    }

    #[test]
    fn a_missing_file_has_no_reply() {
        let mut transcript = Transcript::new(PathBuf::from("/nonexistent/session.jsonl"));

        transcript.update();

        assert_eq!(transcript.reply, None);
    }
}

mod prompts {
    use super::*;

    const AT: &str = "2026-10-08T20:23:15.694Z";

    fn at() -> Option<SystemTime> {
        Stamp::parse(AT)
    }

    fn prompted(lines: &[&str]) -> Option<SystemTime> {
        let mut file = Written::new();
        file.lines(&lines.iter().map(|line| line.replace("$AT", AT)).collect::<Vec<_>>());
        file.transcript.prompted
    }

    #[rstest]
    #[case::typed(
        r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"fix the login"},"timestamp":"$AT","origin":{"kind":"human"},"promptSource":"typed"}"#
    )]
    #[case::queued_while_it_works(
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"$AT","sessionId":"s","content":"and add tests"}"#
    )]
    #[case::a_paste_queued(
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"$AT","content":"<pasted_content id=\"ab12\">\nlong\n</pasted_content>"}"#
    )]
    #[case::a_slash_command(
        r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>\n<command-args></command-args>"},"timestamp":"$AT"}"#
    )]
    #[case::a_slash_command_that_opened_a_dialog(
        r#"{"type":"system","content":"<command-name>/model</command-name>","timestamp":"$AT"}"#
    )]
    #[case::a_shell_command(
        r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"<bash-input>ls</bash-input>"},"timestamp":"$AT"}"#
    )]
    fn a_prompt_typed_or_pasted_is_recorded_with_its_time(#[case] line: &str) {
        assert_eq!(prompted(&[line]), at());
    }

    #[rstest]
    #[case::a_reply(&assistant("claude-opus-5-5", 1, 0, 9))]
    #[case::a_tool_result(
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"<command-name>/x"}]},"toolUseResult":{},"timestamp":"$AT"}"#
    )]
    #[case::a_task_notification(
        r#"{"type":"user","message":{"role":"user","content":"<task-notification>done</task-notification>"},"timestamp":"$AT","origin":{"kind":"task-notification"}}"#
    )]
    #[case::a_task_notification_queued(
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"$AT","content":"<task-notification>\n<task-id>b1</task-id>"}"#
    )]
    #[case::another_agent_queued(
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"$AT","content":"<agent-message from=\"x\">hi</agent-message>"}"#
    )]
    #[case::taken_from_the_queue(r#"{"type":"queue-operation","operation":"dequeue","timestamp":"$AT"}"#)]
    #[case::taken_between_two_tools(
        r#"{"type":"queue-operation","operation":"remove","timestamp":"$AT","content":"and add tests","reason":"absorbed_mid_turn"}"#
    )]
    #[case::handed_to_the_model_between_two_tools(
        r#"{"isSidechain":false,"type":"attachment","timestamp":"$AT","attachment":{"type":"queued_command","prompt":"and add tests","commandMode":"prompt","origin":{"kind":"human"},"humanTurn":true}}"#
    )]
    #[case::an_interruption(
        r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"timestamp":"$AT"}"#
    )]
    #[case::a_commands_output(
        r#"{"type":"system","subtype":"local_command","content":"<local-command-stdout>ok</local-command-stdout>","timestamp":"$AT"}"#
    )]
    #[case::a_note_for_the_model(
        r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<command-name>/x"},"timestamp":"$AT","origin":{"kind":"human"}}"#
    )]
    #[case::a_subagents_prompt(
        r#"{"isSidechain":true,"type":"user","message":{"role":"user","content":"look"},"timestamp":"$AT","origin":{"kind":"human"}}"#
    )]
    fn other_lines_are_not_a_prompt(#[case] line: &str) {
        assert_eq!(prompted(&[line]), None);
    }

    #[test]
    fn the_newest_prompt_counts() {
        let line = |at: &str| {
            format!(
                r#"{{"type":"user","message":{{"role":"user","content":"hi"}},"timestamp":"{at}","origin":{{"kind":"human"}}}}"#
            )
        };

        let found = prompted(&[&line("2026-10-08T20:23:15.694Z"), &line("2026-10-08T20:24:00.000Z")]);

        assert_eq!(found, Stamp::parse("2026-10-08T20:24:00.000Z"));
    }

    #[test]
    fn a_prompt_sent_while_a_tool_runs_counts_from_when_it_was_queued() {
        let queued = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-10-08T20:57:35.079Z","content":"and add tests"}"#;
        let taken = [
            r#"{"type":"queue-operation","operation":"remove","timestamp":"2026-10-08T20:57:51.454Z","content":"and add tests","reason":"absorbed_mid_turn"}"#,
            r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":""}]},"toolUseResult":{},"timestamp":"2026-10-08T20:57:51.415Z"}"#,
            r#"{"isSidechain":false,"type":"attachment","timestamp":"2026-10-08T20:57:51.500Z","attachment":{"type":"queued_command","prompt":"and add tests","origin":{"kind":"human"}}}"#,
        ];
        let mut file = Written::new();

        file.lines(&[queued.to_string()]);
        let sent = file.transcript.prompted;
        file.lines(&taken.map(str::to_string));

        assert_eq!((sent, file.transcript.prompted), (Stamp::parse("2026-10-08T20:57:35.079Z"), sent));
    }

    #[test]
    fn a_prompt_without_a_time_counts_from_when_it_was_read() {
        let read = SystemTime::now();

        let found =
            prompted(&[r#"{"type":"user","message":{"role":"user","content":"hi"},"origin":{"kind":"human"}}"#]);

        assert!(found.is_some_and(|at| at >= read), "{found:?}");
    }
}

mod folder {
    use super::*;

    const LONG_ASCII: &str = "-home-ana-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-fold-3mvvc4";

    #[rstest]
    #[case::plain("/home/ubuntu/projects/shop", "-home-ubuntu-projects-shop")]
    #[case::dots_and_dashes("/home/a/.cornercase/worktrees/shop/feat-x", "-home-a--cornercase-worktrees-shop-feat-x")]
    #[case::accents_and_emoji("/home/ana/código/🦀 shop", "-home-ana-c-digo----shop")]
    fn is_the_folder_with_every_other_character_as_a_dash(#[case] cwd: &str, #[case] expected: &str) {
        assert_eq!(project_dir(Path::new(cwd)), expected);
    }

    #[test]
    fn a_long_one_is_cut_and_gets_a_hash() {
        let cwd = format!("/home/ana/{}repo", "very-long-folder-name/".repeat(10));

        assert_eq!(project_dir(Path::new(&cwd)), LONG_ASCII);
    }

    #[test]
    fn a_long_one_counts_characters_like_javascript() {
        let cwd = format!("/home/ana/{}/x", "ñ".repeat(195));

        assert_eq!(project_dir(Path::new(&cwd)), format!("-home-ana-{}-ck6jya", "-".repeat(190)));
    }

    #[test]
    fn holds_the_transcript_of_the_session() {
        let path = transcript_path(Path::new("/c"), Path::new("/home/a/shop"), "a01c-9f");

        assert_eq!(path, Some(PathBuf::from("/c/projects/-home-a-shop/a01c-9f.jsonl")));
    }

    #[rstest]
    #[case::empty("")]
    #[case::path("../../etc/passwd")]
    fn a_strange_session_id_has_none(#[case] id: &str) {
        assert_eq!(transcript_path(Path::new("/c"), Path::new("/home/a/shop"), id), None);
    }
}

mod model {
    use super::*;

    #[rstest]
    #[case::current("claude-opus-5-5", "Opus 5.5")]
    #[case::dated("claude-sonnet-4-5-20250929", "Sonnet 4.5")]
    #[case::major_only("claude-sonnet-4-20250514", "Sonnet 4")]
    #[case::zero_minor("claude-opus-4-0", "Opus 4")]
    #[case::haiku("claude-haiku-4-5-20251001", "Haiku 4.5")]
    #[case::fable("claude-fable-5-1", "Fable 5.1")]
    #[case::version_first("claude-3-5-sonnet-20241022", "Sonnet 3.5")]
    #[case::bedrock("us.anthropic.claude-opus-4-1-20250805-v1:0", "Opus 4.1")]
    #[case::vertex("claude-opus-4-1@20250805", "Opus 4.1")]
    #[case::another_provider("glm-4.6", "glm-4.6")]
    fn is_named_like_claude_names_it(#[case] id: &str, #[case] expected: &str) {
        assert_eq!(model_name(id), expected);
    }
}

mod window {
    use super::*;

    fn limits(model: Option<&str>) -> Limits {
        Limits { model: model.map(str::to_string), ..Limits::default() }
    }

    #[rstest]
    #[case::opus_5_5("claude-opus-5-5", None, LONG_WINDOW)]
    #[case::opus_4_7("claude-opus-4-7", None, LONG_WINDOW)]
    #[case::sonnet_5("claude-sonnet-5", None, LONG_WINDOW)]
    #[case::fable("claude-fable-5-1", None, LONG_WINDOW)]
    #[case::opus_4_6("claude-opus-4-6", None, STANDARD_WINDOW)]
    #[case::sonnet_4_5("claude-sonnet-4-5-20250929", None, STANDARD_WINDOW)]
    #[case::haiku("claude-haiku-4-5-20251001", None, STANDARD_WINDOW)]
    #[case::asked_by_alias("claude-sonnet-4-5-20250929", Some("sonnet[1m]"), LONG_WINDOW)]
    #[case::asked_by_id("claude-sonnet-4-5-20250929", Some("claude-sonnet-4-5[1M]"), LONG_WINDOW)]
    #[case::asked_for_another_model("claude-haiku-4-5-20251001", Some("sonnet[1m]"), STANDARD_WINDOW)]
    #[case::plain_setting("claude-sonnet-4-5-20250929", Some("sonnet"), STANDARD_WINDOW)]
    fn follows_the_model(#[case] model: &str, #[case] setting: Option<&str>, #[case] expected: u64) {
        assert_eq!(limits(setting).window(model, 1000), expected);
    }

    #[test]
    fn more_than_200k_tokens_means_1m() {
        assert_eq!(limits(None).window("claude-sonnet-4-5", 200_001), LONG_WINDOW);
    }

    #[test]
    fn turning_1m_off_keeps_200k() {
        let off = Limits { no_long: true, ..limits(Some("opus[1m]")) };

        assert_eq!(off.window("claude-opus-5-5", 1000), STANDARD_WINDOW);
    }

    #[rstest]
    #[case::unknown_model("glm-4.6", false, 128_000)]
    #[case::claude_model("claude-sonnet-4-5", false, STANDARD_WINDOW)]
    #[case::compacting_off("claude-opus-5-5", true, 128_000)]
    fn a_maximum_counts_for_other_models_or_with_compacting_off(
        #[case] model: &str,
        #[case] no_compact: bool,
        #[case] expected: u64,
    ) {
        let max = Limits { max_tokens: Some(128_000), no_compact, ..Limits::default() };

        assert_eq!(max.window(model, 1000), expected);
    }

    #[rstest]
    #[case::nearly_three_quarters(149_954, STANDARD_WINDOW, 75)]
    #[case::rounds_half_up(1_000, STANDARD_WINDOW, 1)]
    #[case::nothing_much(400, STANDARD_WINDOW, 0)]
    #[case::long(165_611, LONG_WINDOW, 17)]
    #[case::never_above_100(250_000, STANDARD_WINDOW, 100)]
    fn the_percentage_is_rounded_like_claude_does(#[case] tokens: u64, #[case] window: u64, #[case] expected: u16) {
        assert_eq!(percent(tokens, window), expected);
    }
}

mod limits {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> HashMap<String, String> {
        vars.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    }

    #[rstest]
    #[case::flag(&["claude", "--model", "opus[1m]"], &[(MODEL_ENV, "sonnet")], Some("opus[1m]"))]
    #[case::flag_with_equals(&["claude", "--model=haiku"], &[], Some("haiku"))]
    #[case::last_flag(&["claude", "--model", "opus", "--model", "sonnet"], &[], Some("sonnet"))]
    #[case::environment(&["claude"], &[(MODEL_ENV, "sonnet[1m]")], Some("sonnet[1m]"))]
    #[case::empty_environment(&["claude"], &[(MODEL_ENV, " ")], Some("fable"))]
    #[case::settings(&["claude"], &[], Some("fable"))]
    fn the_model_comes_from_the_flag_then_the_environment_then_the_settings(
        #[case] flags: &[&str],
        #[case] vars: &[(&str, &str)],
        #[case] expected: Option<&str>,
    ) {
        let found = Limits::new(&strings(flags), &env(vars), Some("fable".into())).model;

        assert_eq!(found.as_deref(), expected);
    }

    #[rstest]
    #[case::one("1", true)]
    #[case::yes(" YES ", true)]
    #[case::on("on", true)]
    #[case::zero("0", false)]
    #[case::anything_else("please", false)]
    fn switches_take_what_claude_takes(#[case] value: &str, #[case] expected: bool) {
        let found = Limits::new(&[], &env(&[(NO_LONG_ENV, value), (NO_COMPACT_ENV, value)]), None);

        assert_eq!((found.no_long, found.no_compact), (expected, expected));
    }

    #[rstest]
    #[case::number("300000", Some(300_000))]
    #[case::zero("0", None)]
    #[case::text("lots", None)]
    fn the_maximum_must_be_a_positive_number(#[case] value: &str, #[case] expected: Option<u64>) {
        assert_eq!(Limits::new(&[], &env(&[(MAX_TOKENS_ENV, value)]), None).max_tokens, expected);
    }
}

struct Setup {
    claude_dir: TempDir,
    project: TempDir,
    sleeper: Sleeper,
}

impl Setup {
    fn new(vars: &[(&str, &str)]) -> Self {
        Self { claude_dir: TempDir::new(), project: TempDir::new(), sleeper: Sleeper::with_env(vars) }
    }

    fn claude(&self, args: &[&str]) -> Claude {
        let text = serde_json::json!({ "sessionId": "s1", "cwd": self.project.path() }).to_string();
        let session = Session::parse(&text, self.sleeper.pid());
        Claude { pid: self.sleeper.pid(), args: strings(args), session }
    }

    fn limits(&self, args: &[&str]) -> Limits {
        Limits::read(self.claude_dir.path(), self.project.path(), &self.claude(args))
    }

    fn transcript(&self, lines: &[String]) {
        let path = transcript_path(self.claude_dir.path(), self.project.path(), "s1").expect("a transcript path");
        write_file(&path, &joined(lines));
    }
}

mod settings {
    use super::*;

    #[test]
    fn the_project_wins_over_the_user_and_the_local_file_over_both() {
        let s = Setup::new(&[]);
        write_file(&s.claude_dir.path().join("settings.json"), r#"{"model":"haiku"}"#);
        write_file(&s.project.path().join(".claude/settings.json"), r#"{"model":"sonnet"}"#);
        write_file(&s.project.path().join(".claude/settings.local.json"), r#"{"model":"opus[1m]"}"#);

        assert_eq!(s.limits(&["claude"]).model.as_deref(), Some("opus[1m]"));
    }

    #[test]
    fn variables_come_from_claude_and_its_settings() {
        let s = Setup::new(&[(MAX_TOKENS_ENV, "300000"), (NO_LONG_ENV, "0")]);
        write_file(
            &s.claude_dir.path().join("settings.json"),
            r#"{"env":{"CLAUDE_CODE_DISABLE_1M_CONTEXT":"1","DISABLE_COMPACT":true}}"#,
        );

        let found = s.limits(&["claude"]);

        assert_eq!((found.max_tokens, found.no_long, found.no_compact), (Some(300_000), true, false));
    }

    #[test]
    fn a_broken_file_is_skipped() {
        let s = Setup::new(&[(MODEL_ENV, "sonnet[1m]")]);
        write_file(&s.project.path().join(".claude/settings.json"), r#"{"model":"#);

        assert_eq!(s.limits(&["claude"]).model.as_deref(), Some("sonnet[1m]"));
    }
}

mod pane {
    use super::*;

    #[test]
    fn the_record_is_the_one_of_the_agent_asked_for() {
        let place = opencode::Place { database: "/d/opencode.db".into(), id: "ses".into() };
        let replaced = Pane {
            codex: Some(codex::Rollout::new("/c/rollout.jsonl".into())),
            opencode: Some(opencode::Session { place: Some(place), ..opencode::Session::default() }),
            ..Pane::default()
        };

        let found = [agents::OPENCODE, agents::CLAUDE].map(|agent| replaced.record_for(agent));

        assert_eq!(found, [Some(Record::Opencode { database: "/d/opencode.db".into(), session: "ses".into() }), None]);
    }

    fn answered() -> (Setup, Pane) {
        let s = Setup::new(&[]);
        s.transcript(&[assistant("claude-opus-5-5", 2, 15_655, 149_954)]);
        let mut pane = Pane::default();
        pane.update(Some(s.claude_dir.path()), Some(&s.claude(&["claude"])));
        (s, pane)
    }

    #[test]
    fn shows_the_model_and_the_share_of_its_window() {
        let (_s, pane) = answered();

        assert_eq!(pane.context(), Some(&Context { model: "Opus 5.5".into(), percent: Some(17) }));
    }

    #[test]
    fn a_1m_setting_counts_for_an_older_model() {
        let s = Setup::new(&[]);
        s.transcript(&[assistant("claude-sonnet-4-5-20250929", 0, 0, 100_000)]);
        let mut pane = Pane::default();

        pane.update(Some(s.claude_dir.path()), Some(&s.claude(&["claude", "--model", "sonnet[1m]"])));

        assert_eq!(pane.context().and_then(|c| c.percent), Some(10));
    }

    #[test]
    fn a_new_session_starts_empty() {
        let (s, mut pane) = answered();
        let mut cleared = s.claude(&["claude"]);
        cleared.session.as_mut().expect("a session").id = Some("s2".into());

        pane.update(Some(s.claude_dir.path()), Some(&cleared));

        assert_eq!(pane.context(), None);
    }

    #[test]
    fn goes_away_with_claude() {
        let (s, mut pane) = answered();

        pane.update(Some(s.claude_dir.path()), None);

        assert_eq!((pane.context(), pane.prompted()), (None, None));
    }

    #[test]
    fn a_new_conversation_keeps_when_the_last_prompt_was_sent() {
        let s = Setup::new(&[]);
        let typed = r#"{"type":"user","message":{"role":"user","content":"/clear"},"timestamp":"2026-10-08T20:23:15.694Z","origin":{"kind":"human"}}"#;
        s.transcript(&[typed.to_string()]);
        let mut pane = Pane::default();
        pane.update(Some(s.claude_dir.path()), Some(&s.claude(&["claude"])));
        let mut cleared = s.claude(&["claude"]);
        cleared.session.as_mut().expect("a session").id = Some("s2".into());

        pane.update(Some(s.claude_dir.path()), Some(&cleared));

        assert_eq!(pane.prompted(), Stamp::parse("2026-10-08T20:23:15.694Z"));
    }
}

#[expect(unused_qualifications, reason = "preserve the existing test bodies when nesting this module")]
mod counting_records {
    use super::*;
    use crate::agents::context::counting::tests::{since, sum};
    use crate::test_util::{TempDir, wait_until};

    const FIXTURE: &str = crate::test_util::fixture!("claude/2.1.296/usage.jsonl");

    #[rstest::rstest]
    #[case::reply(r#"{"type":"assistant","message":{"usage":{"output_tokens":1}}}"#, true)]
    #[case::typed(r#"{"type":"user", "origin": {"kind":"human"}}"#, true)]
    #[case::absorbed(r#"{"type":"attachment","attachment":{"type":"queued_command"}}"#, true)]
    #[case::queued(r#"{"type":"queue-operation", "operation": "enqueue"}"#, true)]
    #[case::slash(r#"{"type":"system","content":"<command-name>/synthetic"}"#, true)]
    #[case::shell(r#"{"type":"user","message":{"content":"<bash-input>synthetic"}}"#, true)]
    #[case::compacted(r#"{"type":"system","subtype":"compact_boundary"}"#, true)]
    #[case::tool_result(
        r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"synthetic"}]}}"#,
        false
    )]
    #[case::tool_input(
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","input":{"content":"synthetic"}}]}}"#,
        false
    )]
    #[case::metadata(r#"{"type":"file-history-snapshot","snapshot":{}}"#, false)]
    fn only_lines_that_can_change_context_or_counts_reach_json_parsing(#[case] line: &str, #[case] relevant: bool) {
        assert_eq!(relevant_claude(line), relevant);
        assert_eq!(claude_entry(line.as_bytes()).is_some(), relevant);
        let large = line.replace("synthetic", &"x".repeat(300_000));
        assert_eq!(claude_entry(large.as_bytes()).is_some(), relevant);
    }

    #[test]
    fn repeated_message_usage_and_conversation_resets_count_once() {
        let mut ledger = Ledger::default();
        let records: Vec<_> = FIXTURE
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|entry| counted_claude(&entry, true))
            .collect();
        ledger.apply("a", since(), records.clone());
        assert_eq!(
            sum(ledger.take()),
            (2, crate::agents::stats::Tokens { input: 20, out: 50, cache_read: 200, cache_write: 10 })
        );
        ledger.apply("clear", since(), Vec::new());
        ledger.apply("a", since(), records);
        assert_eq!(sum(ledger.take()), (0, crate::agents::stats::Tokens::default()));
        assert!(
            counted_claude(
                &serde_json::json!({"type":"assistant","message":{"id":"missing-time","usage":{"output_tokens":99}}}),
                true
            )
            .is_none()
        );
    }

    #[test]
    fn absorbed_and_dequeued_human_prompts_each_count_once() {
        let fixture = crate::test_util::fixture!("claude/2.1.296/queued-prompts.jsonl");
        let records: Vec<_> = fixture
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("entry"))
            .filter_map(|entry| counted_claude(&entry, true))
            .collect();
        assert_eq!(records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["absorbed", "dequeued"]);
        let mut ledger = Ledger::default();
        ledger.apply("queue", since(), records.clone());
        assert_eq!(sum(ledger.take()).0, 2);
        ledger.apply("queue", since(), records);
        assert_eq!(sum(ledger.take()).0, 0);
    }

    #[test]
    fn a_subagent_adds_tokens_through_its_own_incremental_tail() {
        let dir = TempDir::new();
        let cwd = dir.path().join("synthetic");
        let transcript = transcript_path(dir.path(), &cwd, "session").expect("path");
        let sub = transcript.with_extension("").join("subagents/agent-synthetic.jsonl");
        std::fs::create_dir_all(sub.parent().expect("parent")).expect("directory");
        std::fs::write(&transcript, FIXTURE).expect("transcript");
        std::fs::write(&sub, crate::test_util::fixture!("claude/2.1.296/subagent.jsonl")).expect("subagent");
        let session = crate::agents::activity::Session::parse(
            &serde_json::json!({"pid": 1, "sessionId": "session", "cwd": cwd}).to_string(),
            1,
        )
        .expect("session");
        let claude = Claude { pid: 1, args: Vec::new(), session: Some(session) };
        let mut pane = Pane { agent: Some((1, since())), kind: Some(agents::CLAUDE), ..Pane::default() };
        let mut output = 0;
        wait_until("subagent usage", || {
            pane.update(Some(dir.path()), Some(&claude));
            output += sum(pane.take_counted()).1.out;
            output == 100
        });
        pane.update(Some(dir.path()), Some(&claude));
        assert_eq!(sum(pane.take_counted()).1.out, 0);
        let cleared = transcript_path(dir.path(), &cwd, "clear-session").expect("path");
        std::fs::write(cleared, "").expect("clear transcript");
        let session = crate::agents::activity::Session::parse(
            &serde_json::json!({"pid": 1, "sessionId": "clear-session", "cwd": cwd}).to_string(),
            1,
        )
        .expect("clear session");
        pane.update(Some(dir.path()), Some(&Claude { pid: 1, args: Vec::new(), session: Some(session) }));
        pane.update(Some(dir.path()), Some(&claude));
        assert_eq!(sum(pane.take_counted()).1.out, 0);
    }
}
