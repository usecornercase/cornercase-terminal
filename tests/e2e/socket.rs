use super::*;

struct Home {
    dir: PathBuf,
}

impl Home {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cch-{}-{n}", std::process::id()));
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir.join("home")).expect("create the home");
        Self { dir }
    }

    fn state(&self) -> PathBuf {
        self.dir.join("state")
    }

    fn socket(&self) -> PathBuf {
        self.state().join(own_folder()).join("server.sock")
    }

    fn command(&self, runtime: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_cornercase"));
        for var in [SOCKET_ENV, NESTED_ENV, PANE_ENV, "CORNERCASE_SERVER", update::LATEST_ENV] {
            cmd.env_remove(var);
        }
        cmd.env("HOME", self.dir.join("home"))
            .env("XDG_STATE_HOME", self.state())
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("XDG_RUNTIME_DIR", self.dir.join(runtime))
            .env("TMPDIR", self.dir.join(format!("{runtime}-tmp")))
            .env("SHELL", "/bin/sh")
            .env("PS1", "$ ")
            .env(CLAUDE_DIR_ENV, self.dir.join("claude"))
            .current_dir(temp());
        cmd
    }

    fn says(&self, runtime: &str, args: &[&str]) -> String {
        let out = self.command(runtime).args(args).output().expect("run cornercase");
        assert!(out.status.success(), "cornercase {args:?} failed: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    }

    fn boot(&self, runtime: &str) -> String {
        let report: serde_json::Value = serde_json::from_str(&self.says(runtime, &["status", "--json"])).expect("json");
        report["boot"].as_str().expect("a boot").to_string()
    }

    fn servers(&self) -> usize {
        servers_with(&format!("XDG_STATE_HOME={}", self.state().display()))
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        for _ in 0..2 {
            let _ = self.command("a").arg("kill-server").output();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn every_runtime_folder_reaches_the_same_server() {
    let home = Home::new();
    home.says("a", &["start-server", "--json"]);

    let boot = home.boot("a");

    assert_eq!(home.boot("b"), boot);
    assert_eq!(home.servers(), 1);
    assert!(home.socket().exists());
}

#[cfg(not(debug_assertions))]
#[rstest::rstest]
#[case::runtime_folder("a/cornercase")]
#[case::temp_folder("a-tmp/cornercase-UID")]
fn a_server_of_an_earlier_version_is_found_where_it_was(#[case] folder: &str) {
    let home = Home::new();
    let earlier = home.dir.join(folder.replace("UID", &cornercase::protocol::own_uid().to_string()));
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&earlier).expect("create the folder");
    let earlier = earlier.join("server.sock");
    let started = home.command("a").env(SOCKET_ENV, &earlier).args(["start-server", "--json"]).output();
    assert!(started.expect("run cornercase").status.success());
    let boot = home.boot("a");

    let again: serde_json::Value =
        serde_json::from_str(&home.says("a", &["start-server", "--json"])).expect("a start-server answer");

    assert_eq!(again["already"], true);
    assert_eq!(home.boot("a"), boot);
    assert!(!home.socket().exists());
    assert_eq!(home.servers(), 1);
    home.says("a", &["kill-server"]);
    let out = home.command("a").arg("status").output().expect("run cornercase");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no cornercase server is running"), "{out:?}");
}

#[test]
fn cornercases_own_folder_is_made_private() {
    let home = Home::new();
    let own = home.state().join(own_folder());
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&own).expect("create the folder");
    std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o777)).expect("open the folder to all");

    let out = home.command("a").arg("kill-server").output().expect("run cornercase");

    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("no cornercase server is running"), "{out:?}");
    assert_eq!(std::fs::metadata(&own).expect("metadata").permissions().mode() & 0o777, 0o700);
}

#[test]
fn a_remote_window_finds_the_server_whatever_its_ssh_session_has() {
    let home = Home::new();
    home.says("a", &["start-server", "--json"]);
    let boot = home.boot("a");
    let d = home.dir.display();
    let prefix = format!(
        "env -u CORNERCASE_SOCKET HOME={d}/home XDG_STATE_HOME={d}/state XDG_CONFIG_HOME={d}/config \
         XDG_RUNTIME_DIR={d}/b TMPDIR={d}/b-tmp"
    );
    let path = remote::fake_ssh(&home.dir, &prefix);
    let exe = env!("CARGO_BIN_EXE_cornercase");
    let args = ["remote", "devbox", "--command", exe];
    let session = Session::new();

    let mut app = Harness::run(exe.as_ref(), &args, session, (ROWS, COLS), &[("PATH", &path)]);

    app.wait_for("the remote window shows its shell", |s| s.contains(&first_entry()));
    assert_eq!(home.servers(), 1);
    assert_eq!(home.boot("b"), boot);
}
