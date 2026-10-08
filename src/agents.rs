use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::config::Config;
use crate::state::AgentState;

pub const AUTO: &str = "auto";
pub const CLAUDE: &str = "claude";
pub const CODEX: &str = "codex";
pub const OPENCODE: &str = "opencode";
pub const DEFAULT_TRUST_PROMPT: &str =
    "trust the files|trust this (folder|directory|workspace|repository)|do you trust|yes, proceed";
const TRUST_LINES: usize = 15;
const KNOWN: [(&str, &str); 14] = [
    (CLAUDE, "claude"),
    (CODEX, "codex"),
    ("gemini", "gemini"),
    (OPENCODE, "opencode"),
    ("cursor", "cursor-agent"),
    ("copilot", "copilot"),
    ("amp", "amp"),
    ("droid", "droid"),
    ("pi", "pi"),
    ("qwen", "qwen"),
    ("kimi", "kimi"),
    ("cline", "cline"),
    ("goose", "goose"),
    ("aider", "aider"),
];
type ModeTable = &'static [(&'static str, &'static [&'static str])];

const DEFAULT_MODES: [(&str, ModeTable); 3] = [
    (
        "claude",
        &[
            ("accept edits", &["--permission-mode", "acceptEdits"]),
            ("auto", &["--permission-mode", "auto"]),
            ("plan", &["--permission-mode", "plan"]),
            ("skip permissions (dangerous)", &["--dangerously-skip-permissions"]),
        ],
    ),
    (
        "codex",
        &[
            ("read only", &["--sandbox", "read-only"]),
            ("workspace write", &["--sandbox", "workspace-write"]),
            ("no sandbox, no approvals (dangerous)", &["--dangerously-bypass-approvals-and-sandbox"]),
        ],
    ),
    ("gemini", &[("auto edit", &["--approval-mode", "auto_edit"]), ("yolo (dangerous)", &["--yolo"])]),
];

pub type Mode = (String, Vec<String>);

pub fn kinds(config: &Config) -> Vec<String> {
    let mut kinds: Vec<String> = KNOWN.iter().map(|(kind, _)| (*kind).to_string()).collect();
    for kind in config.agent_commands.keys() {
        if !kinds.contains(kind) {
            kinds.push(kind.clone());
        }
    }
    kinds
}

pub fn command(config: &Config, kind: &str) -> String {
    config
        .agent_commands
        .get(kind)
        .cloned()
        .or_else(|| KNOWN.iter().find(|(k, _)| *k == kind).map(|(_, bin)| (*bin).to_string()))
        .unwrap_or_else(|| kind.to_string())
}

fn executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn found(program: &str, path: Option<&OsStr>) -> bool {
    if program.contains('/') {
        return executable(Path::new(program));
    }
    path.is_some_and(|path| std::env::split_paths(path).any(|dir| dir.is_absolute() && executable(&dir.join(program))))
}

pub fn installed(program: &str) -> bool {
    found(program, std::env::var_os("PATH").as_deref())
}

pub fn modes(config: &Config, kind: &str) -> Vec<Mode> {
    let mut modes: Vec<Mode> = DEFAULT_MODES
        .iter()
        .filter(|(k, _)| *k == kind)
        .flat_map(|(_, modes)| modes.iter())
        .map(|(name, args)| ((*name).to_string(), args.iter().map(|a| (*a).to_string()).collect()))
        .collect();
    for (name, args) in config.agent_modes.get(kind).into_iter().flatten() {
        match modes.iter_mut().find(|(n, _)| n == name) {
            Some(mode) => mode.1.clone_from(args),
            None => modes.push((name.clone(), args.clone())),
        }
    }
    modes.retain(|(_, args)| !args.is_empty());
    modes
}

pub fn args(config: &Config, kind: &str) -> Vec<String> {
    config.agent_args.get(kind).cloned().unwrap_or_default()
}

fn position_of(args: &[String], run: &[String]) -> Option<usize> {
    (!run.is_empty()).then(|| args.windows(run.len()).position(|w| w == run)).flatten()
}

fn longest_first(modes: &[Mode]) -> Vec<&Mode> {
    let mut sorted: Vec<&Mode> = modes.iter().collect();
    sorted.sort_by_key(|(_, args)| std::cmp::Reverse(args.len()));
    sorted
}

pub fn mode_of(args: &[String], modes: &[Mode]) -> Option<String> {
    longest_first(modes).into_iter().find(|(_, run)| position_of(args, run).is_some()).map(|(name, _)| name.clone())
}

pub fn extra_args(args: &[String], modes: &[Mode]) -> Vec<String> {
    let mut out = args.to_vec();
    for (_, run) in longest_first(modes) {
        while let Some(at) = position_of(&out, run) {
            out.drain(at..at + run.len());
        }
    }
    out
}

fn mode_args(modes: &[Mode], name: Option<&str>) -> Vec<String> {
    name.and_then(|name| modes.iter().find(|(n, _)| n == name)).map(|(_, args)| args.clone()).unwrap_or_default()
}

pub fn with_mode(args: &[String], modes: &[Mode], name: Option<&str>) -> Vec<String> {
    [mode_args(modes, name), extra_args(args, modes)].concat()
}

pub fn with_extra(args: &[String], modes: &[Mode], extra: Vec<String>) -> Vec<String> {
    [mode_args(modes, mode_of(args, modes).as_deref()), extra].concat()
}

pub fn is_dangerous(mode: &str) -> bool {
    mode.to_lowercase().contains("danger")
}

pub fn split_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"') | None, '\\') => current.get_or_insert_default().extend(chars.next()),
            (None, c) if c.is_whitespace() => out.extend(current.take()),
            (None, '"' | '\'') => {
                quote = Some(c);
                current.get_or_insert_default();
            }
            (_, c) => current.get_or_insert_default().push(c),
        }
    }
    out.extend(current);
    out
}

pub fn quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

pub fn join_args(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

pub fn command_line(config: &Config, kind: &str) -> String {
    std::iter::once(quote(&command(config, kind)))
        .chain(args(config, kind).iter().map(|a| quote(a)))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn resume_line(config: &Config, agent: &AgentState) -> Option<String> {
    let kind = agent.kind.as_str();
    let args = with_mode(&args(config, kind), &modes(config, kind), agent.mode.as_deref());
    let conversation = agent.conversation.clone();
    let line = match kind {
        CLAUDE => [args, vec!["--resume".into(), conversation]].concat(),
        CODEX => [vec!["resume".into()], args, vec![conversation]].concat(),
        _ => return None,
    };
    Some(std::iter::once(command(config, kind)).chain(line).map(|a| quote(&a)).collect::<Vec<_>>().join(" "))
}

pub fn resolve(config: &Config, explicit: Option<&str>, running: Option<&str>) -> Option<String> {
    explicit
        .or_else(|| (config.agent != AUTO && !config.agent.trim().is_empty()).then_some(config.agent.as_str()))
        .or(running)
        .map(str::to_string)
}

fn basename(arg: &str) -> &str {
    Path::new(arg).file_name().and_then(|n| n.to_str()).unwrap_or(arg)
}

pub fn detect(config: &Config, argv: &[String]) -> Option<String> {
    let names: Vec<&str> = argv.iter().take(2).map(|a| basename(a)).collect();
    kinds(config).into_iter().find(|kind| {
        let bin = command(config, kind);
        names.contains(&basename(&bin))
            || (kind == CODEX && argv.get(1).is_some_and(|arg| arg.ends_with("/codex/bin/codex.js")))
    })
}

#[derive(Debug, Default)]
pub struct TrustPrompt {
    compiled: Option<(String, Option<regex::Regex>)>,
}

impl TrustPrompt {
    pub fn regex(&mut self, pattern: &str) -> Option<&regex::Regex> {
        if self.compiled.as_ref().is_none_or(|(compiled, _)| compiled != pattern) {
            let regex = regex::RegexBuilder::new(pattern).case_insensitive(true).build().ok();
            self.compiled = Some((pattern.to_string(), regex));
        }
        self.compiled.as_ref().and_then(|(_, regex)| regex.as_ref())
    }
}

pub fn asks_trust(regex: &regex::Regex, screen: &str) -> bool {
    let lines: Vec<&str> = screen.lines().filter(|line| !line.trim().is_empty()).collect();
    regex.is_match(&lines[lines.len().saturating_sub(TRUST_LINES)..].join("\n"))
}

#[cfg(test)]
mod tests {
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
            assert_eq!(mode_of(&args, &modes(&config(), "claude")).as_deref(), Some("plan"));
        }

        #[test]
        fn extra_arguments_are_the_rest() {
            let args = strings(&["--add-dir", "x", "--permission-mode", "plan"]);
            assert_eq!(extra_args(&args, &modes(&config(), "claude")), strings(&["--add-dir", "x"]));
        }

        #[test]
        fn picking_a_mode_replaces_the_old_one_and_keeps_the_extras() {
            let args = strings(&["--permission-mode", "plan", "--add-dir", "x"]);
            let next = with_mode(&args, &modes(&config(), "claude"), Some("skip permissions (dangerous)"));
            assert_eq!(next, strings(&["--dangerously-skip-permissions", "--add-dir", "x"]));
        }

        #[test]
        fn no_mode_keeps_only_the_extras() {
            let args = strings(&["--permission-mode", "plan", "-v"]);
            assert_eq!(with_mode(&args, &modes(&config(), "claude"), None), strings(&["-v"]));
        }

        #[test]
        fn new_extras_keep_the_mode() {
            let args = strings(&["--sandbox", "read-only", "-v"]);
            let next = with_extra(&args, &modes(&config(), "codex"), strings(&["--model", "o3"]));
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
            assert_eq!(
                (command(&c, "mine"), kinds(&c).contains(&"mine".to_string())),
                ("/opt/bin/my-agent".into(), true)
            );
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
            assert_eq!(resume_line(&config(), &agent("gemini", None)), None);
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
}
