use super::*;
use crate::test_util::TempDir;

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(ToString::to_string).collect()
}

#[test]
fn it_records_its_arguments_prints_its_answers_and_exits_with_its_code() {
    let dir = TempDir::new();
    let gh = dir.path().join("gh");
    std::fs::write(gh.with_extension("out"), "{\"state\":\"OPEN\"}").expect("write the answer");
    std::fs::write(gh.with_extension("err"), "warning").expect("write the warning");
    std::fs::write(gh.with_extension("code"), "3\n").expect("write the code");
    let (mut out, mut err) = (Vec::new(), Vec::new());

    let code = run(&gh, &args(&["pr", "view", "12"]), &mut out, &mut err);
    run(&gh, &args(&["api", "user"]), &mut Vec::new(), &mut Vec::new());

    let recorded = std::fs::read_to_string(gh.with_extension("args")).expect("read the arguments");
    assert_eq!((code, out.as_slice(), err.as_slice()), (3, b"{\"state\":\"OPEN\"}".as_slice(), b"warning".as_slice()));
    assert_eq!(recorded, "pr view 12\napi user\n");
}

#[test]
fn it_records_where_each_argument_ends() {
    let dir = TempDir::new();
    let ssh = dir.path().join("ssh");

    run(&ssh, &args(&["a b", "", "c"]), &mut Vec::new(), &mut Vec::new());

    let recorded = std::fs::read_to_string(ssh.with_extension("argv")).expect("read the arguments");
    assert_eq!(recorded, "[\"a b\", \"\", \"c\"]\n");
}

#[test]
fn with_nothing_beside_it_it_prints_nothing_and_succeeds() {
    let dir = TempDir::new();
    let (mut out, mut err) = (Vec::new(), Vec::new());

    let code = run(&dir.path().join("glab"), &[], &mut out, &mut err);

    assert_eq!((code, out.is_empty(), err.is_empty()), (0, true, true));
}

#[test]
fn it_records_the_variables_it_is_asked_about_and_says_which_are_unset() {
    let dir = TempDir::new();
    let claude = dir.path().join("claude");
    std::fs::write(claude.with_extension("vars"), "PATH CORNERCASE_NOT_SET_ANYWHERE\n").expect("write the names");

    run(&claude, &[], &mut Vec::new(), &mut Vec::new());

    let recorded = std::fs::read_to_string(claude.with_extension("env")).expect("read the variables");
    let mut lines = recorded.lines();
    assert!(lines.next().is_some_and(|line| line.starts_with("PATH=")), "{recorded}");
    assert_eq!(lines.next(), Some("CORNERCASE_NOT_SET_ANYWHERE"));
}

#[test]
fn what_comes_after_the_hold_is_printed_once_it_is_released() {
    let dir = TempDir::new();
    let claude = dir.path().join("claude");
    std::fs::write(claude.with_extension("out"), "before ").expect("write the first answer");
    std::fs::write(claude.with_extension("after"), "after").expect("write the last answer");
    let mut out = Vec::new();

    run(&claude, &[], &mut out, &mut Vec::new());

    assert_eq!(String::from_utf8_lossy(&out), "before after");
}
