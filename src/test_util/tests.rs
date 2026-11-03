use std::process::Command;

use super::*;

mod fake_program {
    use super::*;

    #[test]
    fn is_found_on_the_path_by_its_name_and_answers() {
        let dir = TempDir::new();
        let gh = fake_program(dir.path(), "gh");
        std::fs::write(gh.with_extension("out"), "fake gh").expect("write the answer");
        std::fs::write(gh.with_extension("code"), "4").expect("write the code");

        let output = Command::new("gh").args(["auth", "status"]).env("PATH", dir.path()).output().expect("run gh");

        let recorded = std::fs::read_to_string(gh.with_extension("args")).expect("read the arguments");
        assert_eq!((output.status.code(), output.stdout.as_slice()), (Some(4), b"fake gh".as_slice()));
        assert_eq!(recorded, "auth status\n");
    }

    #[test]
    fn runs_under_its_own_name_while_held() {
        let dir = TempDir::new();
        let claude = fake_program(dir.path(), "claude");
        std::fs::write(claude.with_extension("hold"), "").expect("hold it");
        let mut child = Command::new(&claude).spawn().expect("start the fake claude");
        let pid = i32::try_from(child.id()).expect("pid fits in i32");

        wait_until("it runs as claude", || crate::process::name(pid).as_deref() == Some("claude"));
        std::fs::remove_file(claude.with_extension("hold")).expect("release it");

        wait_until("it exits once released", || child.try_wait().expect("check the fake").is_some());
    }
}

#[test]
fn a_written_executable_runs() {
    let dir = TempDir::new();
    let (name, script) =
        if cfg!(windows) { ("hello.cmd", "@echo hello %1\r\n") } else { ("hello", "#!/bin/sh\necho hello \"$1\"\n") };
    write_executable(&dir.path().join(name), script);

    let output = Command::new(dir.path().join(name)).arg("there").output().expect("run the script");

    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), "hello there");
}
