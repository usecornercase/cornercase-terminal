use rstest::rstest;

use super::*;

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

#[rstest]
#[case::direct(&["/usr/local/bin/gemini"], true)]
#[case::parent(&["node", "/usr/local/bin/gemini"], true)]
#[case::child(&["/usr/local/bin/node", "--max-old-space-size=8192", "/usr/local/bin/gemini"], true)]
#[case::argument(&["cat", "--", "/tmp/gemini"], false)]
#[case::other_script(&["node", "--max-old-space-size=8192", "/tmp/other", "/tmp/gemini"], false)]
fn a_node_child_is_recognized_past_its_runtime_flags(#[case] args: &[&str], #[case] expected: bool) {
    assert_eq!(named(&strings(args), "gemini"), expected);
}

#[rstest]
#[case::the_package_script("/usr/lib/node_modules/@openai/codex/bin/codex.js", true)]
#[case::another_package("/usr/lib/node_modules/@openai/other/bin/codex.js", false)]
#[case::too_few_folders("bin/codex.js", false)]
fn an_npm_script_is_matched_by_its_last_folders(#[case] arg: &str, #[case] expected: bool) {
    assert_eq!(npm_script(arg, &["codex", "bin", "codex.js"]), expected);
}

#[test]
fn a_variable_is_named_in_any_case_only_on_windows_and_never_empty() {
    let env = vec![("CODEX_HOME".to_string(), String::new()), ("Codex_Home".to_string(), "x".to_string())];
    assert_eq!(variable(&env, "CODEX_HOME"), if cfg!(windows) { Some("x") } else { None });
    assert_eq!(variable(&env, "Codex_Home"), Some("x"));
    assert_eq!(variable_key("Codex_Home".to_string()), if cfg!(windows) { "CODEX_HOME" } else { "Codex_Home" });
}

#[cfg(windows)]
mod on_windows {
    use std::process::{Command, Stdio};

    use super::*;
    use crate::test_util::{Running, TempDir, wait_until};

    #[rstest]
    #[case::node_with_a_launcher(&[r"C:\Program Files\nodejs\node.exe", r"C:\npm\node_modules\opencode-ai\bin\opencode"], true)]
    #[case::a_binary_with_its_exe(&[r"C:\npm\node_modules\opencode-ai\bin\opencode.EXE"], true)]
    #[case::node_with_another_script(&[r"C:\Program Files\nodejs\node.exe", r"C:\npm\other.js"], false)]
    fn a_program_is_named_without_its_exe(#[case] args: &[&str], #[case] expected: bool) {
        assert_eq!(named(&strings(args), "opencode"), expected);
    }

    #[rstest]
    #[case::backslashes(r"C:\Users\a\AppData\Roaming\npm\\node_modules\@openai\codex\bin\codex.js", true)]
    #[case::forward_slashes("C:/Users/a/AppData/Roaming/npm/node_modules/@openai/codex/bin/codex.js", true)]
    #[case::another_package(r"C:\npm\node_modules\@openai\other\bin\codex.js", false)]
    fn an_npm_script_is_matched_with_either_separator(#[case] arg: &str, #[case] expected: bool) {
        assert_eq!(npm_script(arg, &["codex", "bin", "codex.js"]), expected);
    }

    #[test]
    fn an_agents_home_is_in_its_process_profile_and_home_is_not_read() {
        let (profile, elsewhere) = (TempDir::new(), TempDir::new());
        for dir in [&profile, &elsewhere] {
            std::fs::create_dir(dir.path().join(".codex")).expect("make .codex");
        }
        let ping = Running::spawn(
            Command::new("ping")
                .args(["-n", "30", "127.0.0.1"])
                .env("USERPROFILE", profile.path())
                .env("HOME", elsewhere.path())
                .env_remove("CODEX_HOME")
                .stdout(Stdio::null()),
        );
        wait_until("ping's environment is readable", || {
            process::env(ping.pid()).iter().any(|(key, _)| key == "USERPROFILE")
        });

        let found = home(ping.pid(), "CODEX_HOME", "", ".codex");
        assert_eq!(found, Some(profile.path().join(".codex").canonicalize().expect("canonical")));
    }
}
