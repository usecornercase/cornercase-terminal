use std::process::Command;

use super::*;

mod temp_dir {
    use super::*;

    #[test]
    fn skips_a_folder_a_dead_process_left_behind() {
        let base = TempDir::new();
        std::fs::create_dir(base.path().join("stem-0")).expect("a leftover");
        std::fs::create_dir(base.path().join("stem-1")).expect("a leftover");
        let mut counter = 0..;

        let dir = fresh_dir(base.path(), "stem", || counter.next().expect("a number"));

        assert_eq!(dir, base.path().join("stem-2"));
        assert!(dir.is_dir());
    }
}

mod free_port {
    use super::*;

    #[test]
    fn gives_a_different_port_each_time() {
        let (a, b) = (free_port(), free_port());

        assert_ne!(a, b);
    }
}

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

    #[test]
    fn listens_on_the_forward_it_is_given_and_fails_like_ssh_when_it_is_taken() {
        let dir = TempDir::new();
        let ssh = fake_program(dir.path(), "ssh");
        std::fs::write(ssh.with_extension("forward"), "").expect("make it listen");
        let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("take a port");
        let port = taken.local_addr().expect("the port").port();

        let output = Command::new(&ssh).args(["-L", &format!("127.0.0.1:{port}:localhost:80")]).output().expect("run");

        assert_eq!(output.status.code(), Some(255));
        assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("bind 127.0.0.1:{port}")));
    }

    #[test]
    fn is_built_beside_the_test_binary_rather_than_in_the_shared_temp_folder() {
        let test_binary = std::env::current_exe().expect("the test binary's path");

        assert_eq!(built_program().parent(), test_binary.parent());
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
