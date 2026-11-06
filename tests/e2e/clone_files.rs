use serde_json::{Value, json};

use super::remote::{remote_says, remotely, ssh_runs};
use super::*;

const IGNORED: &str = ".env\nweb/.env.local\ndump.sql\n";
const TOO_BIG: usize = 1024 * 1024 + 1;

struct Shop {
    here: PathBuf,
    url: String,
}

fn put(repo: &std::path::Path, path: &str, contents: &[u8], mode: u32) {
    let path = repo.join(path);
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("create the folder");
    std::fs::write(&path, contents).expect("write the file");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

fn shop(session: &Session) -> Shop {
    let here = repository(&session.dir, "shop");
    put(&here, ".gitignore", IGNORED.as_bytes(), 0o644);
    put(&here, "README.md", b"the repository's\n", 0o644);
    git(&here, &["add", "--all"]);
    git(&here, &["commit", "--quiet", "-m", "a readme"]);
    put(&here, ".env", b"KEY=1\n", 0o600);
    put(&here, "web/.env.local", b"LOCAL=1\n", 0o640);
    put(&here, "dump.sql", &vec![b'x'; TOO_BIG], 0o644);
    let bare = session.dir.join("shop.git");
    git(&session.dir, &["clone", "--quiet", "--bare", &here.display().to_string(), &bare.display().to_string()]);
    Shop { here, url: bare.display().to_string() }
}

fn arrived(path: &std::path::Path) -> (String, u32) {
    let mode = std::fs::metadata(path).expect("the file arrived").permissions().mode() & 0o777;
    (std::fs::read_to_string(path).expect("read the file"), mode)
}

#[test]
fn files_read_here_reach_the_clone_on_the_other_machine_with_their_permissions() {
    let session = Session::new();
    let shop = shop(&session);
    remote_says(&session, &["start-server"]);
    let (parent, from) = (session.dir.join("there").display().to_string(), shop.here.display().to_string());
    let mut args = vec!["clone", &shop.url, &parent, "--from", &from, "--json"];
    args.extend(["--file", ".env", "--file", "web/.env.local", "--file", "README.md"]);

    let out = remotely(&session, &args).output().expect("run cornercase");

    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let answer: Value = serde_json::from_slice(&out.stdout).expect("the whole answer");
    assert_eq!(answer["unwritten"], json!([{"path": "README.md", "why": "it is already in the clone"}]));
    let clone = PathBuf::from(answer["path"].as_str().expect("its path"));
    assert_eq!(arrived(&clone.join(".env")), ("KEY=1\n".to_string(), 0o600));
    assert_eq!(arrived(&clone.join("web/.env.local")), ("LOCAL=1\n".to_string(), 0o640));
    assert_eq!(arrived(&clone.join("README.md")).0, "the repository's\n");
    assert!(session.report().projects.iter().any(|p| p.path == clone), "the project is open there");
}

#[test]
fn all_that_a_clone_takes_crosses_the_link_in_one_request() {
    let session = Session::new();
    let shop = shop(&session);
    remote_says(&session, &["start-server"]);
    let names: Vec<String> = (0..8).map(|n| format!("{n}.bin")).collect();
    let (parent, from) = (session.dir.join("there").display().to_string(), shop.here.display().to_string());
    let mut args = vec!["clone", &shop.url, &parent, "--from", &from];
    for name in &names {
        put(&shop.here, name, &vec![b'x'; 1024 * 1024], 0o644);
        args.extend(["--file", name]);
    }

    let cloned = PathBuf::from(remote_says(&session, &args));

    for name in &names {
        assert_eq!(std::fs::metadata(cloned.join(name)).expect("the file arrived").len(), 1024 * 1024);
    }
}

#[test]
fn a_clone_in_text_prints_its_path_and_names_the_files_it_could_not_write() {
    let session = Session::new();
    let shop = shop(&session);
    session.says(&["start-server"]);
    let parent = session.dir.join("src").display().to_string();
    let args = ["clone", &shop.url, &parent, "--file", "web/.env.local", "--file", "README.md"];

    let out = session.command().args(args).current_dir(shop.here.join("web")).output().expect("run cornercase");

    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let cloned = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim_end());
    assert_eq!(arrived(&cloned.join("web/.env.local")).0, "LOCAL=1\n");
    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(said, "not copied: README.md (it is already in the clone)\n");
}

#[test]
fn the_files_it_can_copy_are_listed_here_without_a_server_or_ssh() {
    let session = Session::new();
    let shop = shop(&session);
    let args = ["clone", "--list-ignored", "--from", &shop.here.display().to_string(), "--json"];

    let local = session.says(&args);
    let far = remotely(&session, &args).output().expect("run cornercase");

    assert!(far.status.success(), "{far:?}");
    assert_eq!(String::from_utf8_lossy(&far.stdout).trim_end(), local);
    let listed: Value = serde_json::from_str(&local).expect("json");
    assert_eq!(listed["root"], shop.here.canonicalize().expect("canonical").display().to_string());
    assert_eq!(
        listed["files"],
        json!([
            {"path": ".env", "size": 6, "mode": 0o600, "too_big": false},
            {"path": "dump.sql", "size": TOO_BIG, "mode": 0o644, "too_big": true},
            {"path": "web/.env.local", "size": 8, "mode": 0o640, "too_big": false},
        ])
    );
    assert_eq!(listed["left_out"], 0);
    assert_eq!((session.servers(), ssh_runs(&session)), (0, 0));
}

#[rstest]
#[case::going_up("../outside.env", 2, "cannot copy `../outside.env`: it goes up with `..`")]
#[case::absolute("/etc/passwd", 2, "cannot copy `/etc/passwd`: it is not relative to the project")]
#[case::into_git(".git/config", 2, "cannot copy `.git/config`: it goes into `.git`")]
#[case::missing("no.env", 1, "cannot copy `no.env`: it is not in `")]
#[case::a_folder("web", 1, "cannot copy `web`: it is a folder")]
#[case::a_link("linked.env", 1, "cannot copy `linked.env`: it is a link")]
#[case::too_big("dump.sql", 1, "cannot copy `dump.sql`: it is over the 1 MB a file can be")]
fn a_file_that_cannot_go_is_refused_by_name_before_the_server_is_asked(
    #[case] path: &str,
    #[case] status: i32,
    #[case] said: &str,
) {
    let session = Session::new();
    let shop = shop(&session);
    std::os::unix::fs::symlink(".env", shop.here.join("linked.env")).expect("symlink");
    let parent = session.dir.join("there");
    let url = "https://ana:s3cret@example.com/shop.git";

    let args =
        ["clone", url, &parent.display().to_string(), "--file", path, "--from", &shop.here.display().to_string()];
    let out = session.cli(&args);

    assert!(refused(&out, status, said), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("s3cret"), "{out:?}");
    assert!(!parent.exists(), "nothing was cloned");
}

#[test]
fn it_says_which_repository_is_missing_with_status_2() {
    let session = Session::new();

    let out = session.cli(&["clone"]);

    assert!(refused(&out, 2, "<URL>"), "{out:?}");
}
