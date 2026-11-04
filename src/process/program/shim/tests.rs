use std::ffi::OsStr;
use std::path::Component;

use rstest::rstest;

use super::*;
use crate::test_util::{NPM_SCRIPT, NPM_SHIM, TempDir, npm_install, under};

const PROGRAM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL \
                       :find_dp0\r\n\"%dp0%\\node_modules\\x\\bin\\x.exe\"   %*\r\n";
const EXE: [&str; 4] = ["node_modules", "x", "bin", "x.exe"];

fn folder(shim: &str, files: &[&[&str]]) -> TempDir {
    let dir = TempDir::new();
    npm_install(dir.path(), shim, files);
    dir
}

fn read(dir: &TempDir, node: Option<&Path>) -> Option<Vec<PathBuf>> {
    target(&dir.path().join("claude.cmd"), |name| node.filter(|_| name == "node").map(Path::to_path_buf))
}

#[test]
fn npm_runs_its_script_with_the_node_beside_the_shim() {
    let dir = folder(NPM_SHIM, &[&NPM_SCRIPT, &["node.exe"]]);
    let elsewhere = PathBuf::from("/opt/node/node.exe");
    let expected = vec![dir.path().join("node.exe"), under(dir.path(), &NPM_SCRIPT)];
    assert_eq!(read(&dir, Some(&elsewhere)), Some(expected));
}

#[test]
fn without_one_beside_it_npm_runs_the_node_powershell_finds() {
    let dir = folder(NPM_SHIM, &[&NPM_SCRIPT]);
    let node = PathBuf::from("/opt/node/NODE.EXE");
    assert_eq!(read(&dir, Some(&node)), Some(vec![node, under(dir.path(), &NPM_SCRIPT)]));
}

#[rstest]
#[case::nowhere(None)]
#[case::a_batch_file(Some(r"C:\nvm\node.cmd"))]
#[case::a_powershell_script(Some(r"C:\nvm\node.ps1"))]
fn a_node_that_is_not_a_program_is_not_used(#[case] node: Option<&str>) {
    let dir = folder(NPM_SHIM, &[&NPM_SCRIPT]);
    assert_eq!(read(&dir, node.map(Path::new)), None);
}

#[test]
fn a_shim_in_node_modules_bin_goes_up_to_the_package() {
    let dir = folder(NPM_SHIM, &[&NPM_SCRIPT]);
    let bin = under(dir.path(), &["node_modules", ".bin"]);
    std::fs::create_dir_all(&bin).expect("make the bin folder");
    std::fs::write(bin.join("claude.cmd"), NPM_SHIM.replace(r"%dp0%\node_modules", r"%dp0%\..")).expect("write");
    let node = PathBuf::from("/opt/node/node.exe");
    let found = target(&bin.join("claude.cmd"), |_| Some(node.clone()));
    assert_eq!(found, Some(vec![node, under(dir.path(), &NPM_SCRIPT)]));
}

#[test]
fn a_target_cmd_would_expand_is_not_read_even_when_it_is_there() {
    let parts = ["node_modules", "%USERNAME%", "cli.js"];
    let dir = folder(&NPM_SHIM.replace(r"@anthropic-ai\claude-code", "%USERNAME%"), &[&parts, &["node.exe"]]);
    assert!(under(dir.path(), &parts).is_file());
    assert_eq!(read(&dir, None), None);
}

#[test]
fn a_shim_of_a_program_runs_the_program() {
    let dir = folder(PROGRAM, &[&EXE]);
    assert_eq!(read(&dir, None), Some(vec![under(dir.path(), &EXE)]));
}

#[test]
fn a_shim_of_a_file_that_is_not_a_program_is_not_read() {
    let dir = folder(&PROGRAM.replace("x.exe", "x.js"), &[&["node_modules", "x", "bin", "x.js"]]);
    assert_eq!(read(&dir, None), None);
}

#[rstest]
#[case::a_shim_written_by_hand("@echo off\r\nnode \"%~dp0\\cli.js\" %*\r\n")]
#[case::shebang_arguments(&NPM_SHIM.replace("\"%_prog%\"  ", "\"%_prog%\" --no-warnings "))]
#[case::shebang_variables(&NPM_SHIM.replace("CALL :find_dp0\r\n", "CALL :find_dp0\r\n@SET FOO=bar\r\n"))]
#[case::another_command_after_it(&format!("{NPM_SHIM}calc.exe\r\n"))]
#[case::unix_line_ends(&NPM_SHIM.replace("\r\n", "\n"))]
#[case::a_target_outside_the_shims_folder(&NPM_SHIM.replace(r"%dp0%\node_modules", r"C:\node_modules"))]
#[case::a_missing_script(&NPM_SHIM.replace("cli.js", "gone.js"))]
#[case::pnpm(
    "@SETLOCAL\r\n@IF EXIST \"%~dp0\\node.exe\" (\r\n  \"%~dp0\\node.exe\"  \
     \"%~dp0\\node_modules\\@anthropic-ai\\claude-code\\cli.js\" %*\r\n) ELSE (\r\n  @SET \
     PATHEXT=%PATHEXT:;.JS;=;%\r\n  node  \"%~dp0\\node_modules\\@anthropic-ai\\claude-code\\cli.js\" %*\r\n)\r\n"
)]
fn any_other_shim_is_not_read(#[case] shim: &str) {
    let dir = folder(shim, &[&NPM_SCRIPT, &["node.exe"]]);
    assert_eq!(read(&dir, None), None);
}

#[test]
fn a_shim_that_is_not_there_is_not_read() {
    assert_eq!(target(Path::new("/nonexistent/claude.cmd"), |_| None), None);
}

#[test]
fn a_shim_is_read_up_to_its_limit_and_one_longer_is_not() {
    let pad = format!(r"{}\..\", "x".repeat(usize::try_from(LIMIT).expect("a size") - NPM_SHIM.len() - 4));
    let full = NPM_SHIM.replace(r"%dp0%\node_modules", &format!(r"%dp0%\{pad}node_modules"));
    assert_eq!(u64::try_from(full.len()), Ok(LIMIT));
    let dir = folder(&full, &[&NPM_SCRIPT, &["node.exe"]]);
    assert!(read(&dir, None).is_some());
    let dir = folder(&format!("{full}calc.exe\r\n"), &[&NPM_SCRIPT, &["node.exe"]]);
    assert_eq!(read(&dir, None), None);
}

fn volume(path: &Path) -> Option<Component<'_>> {
    path.components().next().filter(|part| matches!(part, Component::Prefix(_)))
}

fn normal(path: &Path) -> impl Iterator<Item = &OsStr> {
    path.components().filter(|part| matches!(part, Component::Normal(_))).map(Component::as_os_str)
}

#[test]
fn a_shim_named_by_a_relative_path_is_not_read() {
    let dir = folder(NPM_SHIM, &[&NPM_SCRIPT, &["node.exe"]]);
    let shim = dir.path().join("claude.cmd");
    let here = std::env::current_dir().expect("the working folder");
    if volume(&here) != volume(&shim) {
        eprintln!("skipped: no relative path leads from {} to {}", here.display(), shim.display());
        return;
    }
    let up: PathBuf = normal(&here).map(|_| "..").collect();
    let relative = normal(&shim).fold(up, |path, part| path.join(part));
    assert!(relative.is_relative() && relative.is_file(), "{}", relative.display());
    assert!(target(&shim, |_| None).is_some());
    assert_eq!(target(&relative, |_| None), None);
}

#[cfg(unix)]
#[test]
fn a_shim_that_does_not_answer_in_time_is_not_read() {
    let dir = TempDir::new();
    let fifo = dir.path().join("claude.cmd");
    let made = std::process::Command::new("mkfifo").arg(&fifo).status().expect("run mkfifo");
    assert!(made.success(), "mkfifo {}", fifo.display());
    assert_eq!(read_within(&fifo, Duration::from_millis(100)), None);
    drop(std::fs::OpenOptions::new().write(true).open(&fifo).expect("let the reader go"));
}

#[rstest]
#[case::another_drive(r"a\D:\x")]
#[case::a_forward_slash("a/b")]
#[case::a_double_quote("a\"b")]
#[case::a_variable(r"a\%USERNAME%\b")]
#[case::an_empty_part(r"a\\b")]
#[case::this_folder(r"a\.\b")]
#[case::a_control_character("a\u{7}b")]
fn a_part_cmd_reads_otherwise_is_not_joined(#[case] relative: &str) {
    let dir = TempDir::new();
    assert_eq!(joined(dir.path(), relative), None);
}

#[test]
fn a_parent_folder_goes_up_but_never_past_the_root() {
    let dir = TempDir::new();
    assert_eq!(joined(dir.path(), r"a\..\b\c"), Some(dir.path().join("b").join("c")));
    let root = dir.path().ancestors().last().expect("a root");
    assert_eq!(joined(root, r"..\b"), None);
}
