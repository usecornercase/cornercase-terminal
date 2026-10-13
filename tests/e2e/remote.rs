use super::*;

const SSH_PID: &str = "ssh.pid";

fn fake_ssh(session: &Session) -> String {
    let bin = session.dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create the fake ssh's folder");
    let pid = session.dir.join(SSH_PID);
    write_executable(
        &bin.join("ssh"),
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -o) shift 2 ;;\n    --) shift; break ;;\n    \
             -*) shift ;;\n    *) break ;;\n  esac\ndone\nshift\nexec /bin/sh -c \"exec $*\"\n",
            pid.display()
        ),
    );
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default())
}

fn ssh_pid(session: &Session) -> String {
    std::fs::read_to_string(session.dir.join(SSH_PID)).unwrap_or_default().trim().to_string()
}

fn remote_refusal(command: &str) -> std::process::Output {
    let session = Session::new();
    let path = fake_ssh(&session);
    session
        .command()
        .args(["remote", "devbox", "--command", command])
        .env("PATH", path)
        .output()
        .expect("run cornercase")
}

#[test]
fn a_remote_window_comes_back_after_the_connection_drops() {
    let session = Session::new();
    let path = fake_ssh(&session);
    let exe = env!("CARGO_BIN_EXE_cornercase");
    let args = ["remote", "devbox", "--command", exe];
    let mut app = Harness::run(exe.as_ref(), &args, Arc::clone(&session), (ROWS, COLS), &[("PATH", &path)]);
    app.wait_for("the remote window shows its shell", |s| s.contains(&first_entry()));
    app.send(b"kept=yes; echo set-\"\"done\r");
    app.wait_for("the shell ran it", |s| s.contains("set-done"));
    let first = ssh_pid(&session);

    let killed = std::process::Command::new("kill").args(["-9", &first]).status().expect("run kill");

    assert!(killed.success(), "kill failed");
    app.wait_for_raw("the window says it reconnects", |raw| raw.contains("reconnecting to devbox"));
    app.wait_for("the window comes back", |s| !s.contains("reconnecting") && s.contains(&first_entry()));
    assert_ne!(ssh_pid(&session), first, "a new ssh");
    app.send(b"echo \"$kept\"-still\r");
    app.wait_for("the same shell answers", |s| s.contains("yes-still"));
    assert_eq!(session.servers(), 1);
}

#[test]
fn a_remote_of_another_version_says_which_side_to_update() {
    let dir = temp_dir("old-remote");
    let old = dir.join("cornercase");
    write_executable(&old, "#!/bin/sh\necho 'cornercase-proxy 1 0.0.1'\ncat > /dev/null\n");

    let out = remote_refusal(&old.display().to_string());

    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused(&out, 1, "`devbox` runs cornercase 0.0.1"), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("Run `cornercase update` there"), "{out:?}");
}

#[test]
fn a_remote_without_cornercase_says_how_to_point_at_it() {
    let out = remote_refusal("/nonexistent/cornercase");

    assert!(refused(&out, 1, "cornercase was not found on `devbox`"), "{out:?}");
}
