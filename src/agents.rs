use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::config::Config;

pub const AUTO: &str = "auto";
pub const CLAUDE: &str = "claude";
pub const CODEX: &str = "codex";
pub const DEFAULT_TRUST_PROMPT: &str =
    "trust the files|trust this (folder|directory|workspace|repository)|do you trust|yes, proceed";
const KNOWN: [(&str, &str); 14] = [
    (CLAUDE, "claude"),
    (CODEX, "codex"),
    ("gemini", "gemini"),
    ("opencode", "opencode"),
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

pub fn trust_prompt(config: &Config, screen: &str) -> bool {
    config.auto_accept_trust_prompt
        && regex::RegexBuilder::new(&config.trust_prompt_pattern)
            .case_insensitive(true)
            .build()
            .is_ok_and(|re| re.is_match(screen))
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

        #[test]
        fn recognises_claudes_folder_question() {
            assert!(trust_prompt(&config(), "Do you trust the files in this folder?\n❯ 1. Yes, proceed"));
        }

        #[test]
        fn ignores_other_screens() {
            assert!(!trust_prompt(&config(), "> How can I help?"));
        }

        #[test]
        fn can_be_turned_off() {
            let c = Config { auto_accept_trust_prompt: false, ..config() };
            assert!(!trust_prompt(&c, "Do you trust the files in this folder?"));
        }
    }
}
