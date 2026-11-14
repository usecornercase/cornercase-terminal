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
fn it_records_its_process_id_once_per_call() {
    let dir = TempDir::new();
    let tool = dir.path().join("cornercase");

    run(&tool, &[], &mut Vec::new(), &mut Vec::new());
    run(&tool, &[], &mut Vec::new(), &mut Vec::new());

    let recorded = std::fs::read_to_string(tool.with_extension("pid")).expect("read the ids");
    let expected = format!("{0}\n{0}\n", std::process::id());
    assert_eq!(recorded, expected);
}

#[test]
fn it_copies_what_it_reads_to_its_output_until_the_end_only_with_an_echo_file() {
    let mut out = Vec::new();

    echo(std::io::Cursor::new(b"hello\nhi".to_vec()), &mut out);

    assert_eq!(out, b"hello\nhi");
}
