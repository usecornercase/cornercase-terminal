use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

use super::*;
use crate::plugins::{Kind, Supervisor};
use crate::process;
use crate::test_util::{FakeHttp, Sleeper, TempDir, exited_pid, git, git_repo, wait_until, write_executable};
use crate::text::hex;

const SCRIPT: &str = "#!/bin/sh\nexec sleep 60\n";

fn manifest(version: &str, extra: &str) -> String {
    format!(
        "id = \"acme.notes\"\nname = \"Notes\"\nversion = \"{version}\"\npermissions = [\"read\"]\n\n\
         [[service]]\nid = \"sync\"\ncommand = [\"bin/run\", \"sync\"]\n{extra}"
    )
}

struct Remote {
    work: TempDir,
    bare: TempDir,
}

impl Remote {
    fn new(manifest: &str) -> Self {
        let work = git_repo(&[("cornercase-plugin.toml", manifest)]);
        fs::create_dir_all(work.path().join("bin")).expect("bin");
        write_executable(&work.path().join("bin/run"), SCRIPT);
        git(work.path(), &["add", "--all"]);
        git(work.path(), &["commit", "--quiet", "-m", "script"]);
        let bare = TempDir::new();
        git(bare.path(), &["clone", "--quiet", "--bare", &work.path().display().to_string(), "repo.git"]);
        Self { work, bare }
    }

    fn url(&self) -> String {
        self.bare.path().join("repo.git").display().to_string()
    }

    fn origin(&self, reference: Option<&str>) -> Origin {
        Origin { repo: self.url(), url: self.url(), reference: reference.map(str::to_string), subdir: None }
    }

    fn commit(&self, files: &[(&str, &str)]) {
        for (name, contents) in files {
            fs::write(self.work.path().join(name), contents).expect("write");
        }
        git(self.work.path(), &["commit", "--quiet", "-am", "change"]);
        git(self.work.path(), &["push", "--quiet", &self.url(), "HEAD:main"]);
    }

    fn head(&self) -> String {
        let out = Command::new("git").args(["-C", &self.url(), "rev-parse", "main"]).output().expect("rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}

fn places(home: &TempDir) -> Places {
    Places::new(&home.path().join("config.json"), &home.path().join("session.json"))
}

fn linked(places: &Places, folder: &TempDir) {
    let text = "id = \"acme.notes\"\nname = \"N\"\nversion = \"1\"\n";
    fs::write(folder.path().join(manifest::FILE), text).expect("manifest");
    let mut registry = Registry::load(&places.registry);
    registry.link(folder.path()).expect("link");
    registry.save().expect("save");
}

fn root(places: &Places) -> PathBuf {
    Registry::load(&places.registry).entries[0].source.root().to_path_buf()
}

fn refused(remote: &Remote) -> String {
    let home = TempDir::new();
    let places = places(&home);
    let fetched = fetch(&remote.origin(None), &places).expect("fetch");
    let error = install(fetched, &places).expect_err("refused");
    assert_eq!((Registry::load(&places.registry).entries.len(), staged(&places)), (0, 0), "{error}");
    error
}

fn staged(places: &Places) -> usize {
    fs::read_dir(places.installs.join(STAGING)).map_or(0, Iterator::count)
}

mod installing {
    use super::*;

    #[test]
    fn a_hidden_character_in_the_source_is_shown_escaped() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let hidden = remote.bare.path().join("evil\u{202e}lmth.git");
        fs::rename(remote.bare.path().join("repo.git"), &hidden).expect("rename");
        let url = hidden.display().to_string();
        let origin = Origin { repo: url.clone(), url, reference: None, subdir: None };
        let preview = fetch(&origin, &places(&home)).expect("fetch").preview().join("\n");
        assert!(preview.contains("evil\\u{202e}lmth.git") && !preview.contains('\u{202e}'), "{preview}");
        let gone = Origin { reference: Some("nope\u{202e}".into()), ..origin };
        let error = fetch(&gone, &places(&home)).map(|_| ()).expect_err("no such ref");
        assert!(!error.contains('\u{202e}'), "{error}");
    }

    #[test]
    fn a_plugin_from_a_bare_repository_lands_turned_off_at_its_commit() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        let fetched = fetch(&remote.origin(None), &places).expect("fetch");
        let preview = fetched.preview().join("\n");
        assert!(preview.contains(&remote.head()), "{preview}");
        assert!(preview.contains("service sync: bin/run sync"), "{preview}");
        assert!(preview.contains("asks for read"), "{preview}");
        assert!(preview.contains("It is installed turned off."), "{preview}");
        let installed = install(fetched, &places).expect("install");
        let entry = Registry::load(&places.registry).entries[0].clone();
        let root = entry.source.root().to_path_buf();
        assert_eq!(
            (installed.enabled, installed.previous, root.clone(), root.join(".git").exists(), staged(&places)),
            (false, None, places.installs.join("acme.notes").join(&remote.head()[..12]), false, 0)
        );
        assert!(Manifest::load(&root).is_ok());
    }

    #[test]
    fn installing_again_at_the_same_commit_records_the_new_ref_and_keeps_it_on() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        install(fetch(&remote.origin(None), &places).expect("fetch"), &places).expect("install");
        let mut registry = Registry::open(&places.registry).expect("open");
        registry.set_enabled("acme.notes", true).expect("on");
        registry.save().expect("save");
        git(remote.work.path(), &["tag", "stable"]);
        git(remote.work.path(), &["push", "--quiet", &remote.url(), "stable"]);
        let done = install(fetch(&remote.origin(Some("stable")), &places).expect("fetch"), &places).expect("again");
        let entries = Registry::load(&places.registry).entries;
        let Source::Git { reference, commit, path, .. } = &entries[0].source else { panic!("a git source") };
        assert_eq!(
            (entries.len(), entries[0].enabled, done.enabled, reference.as_deref(), commit.as_str()),
            (1, true, true, Some("stable"), remote.head().as_str())
        );
        assert_eq!((path.join("cornercase-plugin.toml").exists(), staged(&places)), (true, 0));
    }

    #[test]
    fn a_cancelled_preview_leaves_nothing_behind() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        drop(fetch(&remote.origin(None), &places).expect("fetch"));
        assert_eq!((staged(&places), Registry::load(&places.registry).entries.len()), (0, 0));
    }

    #[test]
    fn a_tag_pins_its_commit() {
        let remote = Remote::new(&manifest("1.0", ""));
        git(remote.work.path(), &["tag", "v1"]);
        git(remote.work.path(), &["push", "--quiet", &remote.url(), "v1"]);
        let tagged = remote.head();
        remote.commit(&[("cornercase-plugin.toml", &manifest("2.0", ""))]);
        let home = TempDir::new();
        let fetched = fetch(&remote.origin(Some("v1")), &places(&home)).expect("fetch");
        assert_eq!((fetched.commit.as_str(), fetched.manifest.version.as_str()), (tagged.as_str(), "1.0"));
    }

    #[test]
    fn a_commit_is_never_taken_for_a_branch_or_tag_of_that_name() {
        let remote = Remote::new(&manifest("1.0", ""));
        let reviewed = remote.head();
        remote.commit(&[("cornercase-plugin.toml", &manifest("6.6", ""))]);
        for kind in ["refs/heads", "refs/tags"] {
            let name = format!("{kind}/{reviewed}");
            git(remote.work.path(), &["push", "--quiet", &remote.url(), &format!("HEAD:{name}")]);
        }
        let home = TempDir::new();
        let fetched = fetch(&remote.origin(Some(&reviewed)), &places(&home)).expect("fetch");
        assert_eq!((fetched.commit.as_str(), fetched.manifest.version.as_str()), (reviewed.as_str(), "1.0"));
    }

    #[test]
    fn an_unknown_ref_is_refused() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let error = fetch(&remote.origin(Some("nope")), &places(&home)).expect_err("refused");
        assert!(error.contains("no branch, tag or commit `nope`"), "{error}");
    }

    #[test]
    fn an_id_that_is_linked_cannot_be_installed() {
        let (remote, home, folder) = (Remote::new(&manifest("1.0", "")), TempDir::new(), TempDir::new());
        let places = places(&home);
        linked(&places, &folder);
        let error = fetch(&remote.origin(None), &places).expect_err("refused");
        assert!(error.contains("already linked from"), "{error}");
    }

    #[test]
    fn a_program_that_leads_out_of_the_folder_is_refused() {
        let remote = Remote::new(&manifest("1.0", ""));
        fs::remove_file(remote.work.path().join("bin/run")).expect("remove");
        std::os::unix::fs::symlink("/bin/sh", remote.work.path().join("bin/run")).expect("symlink");
        git(remote.work.path(), &["add", "--all"]);
        remote.commit(&[]);
        let error = refused(&remote);
        assert!(error.contains("outside the plugin's folder"), "{error}");
    }
}

mod building {
    use super::*;

    fn with_make(script: &str) -> Remote {
        let remote = Remote::new(&manifest("1.0", "\n[[build]]\ncommand = [\"./make\"]\n"));
        fs::remove_file(remote.work.path().join("bin/run")).expect("the build makes it");
        write_executable(&remote.work.path().join("make"), script);
        git(remote.work.path(), &["add", "--all"]);
        remote.commit(&[]);
        remote
    }

    #[test]
    fn build_steps_run_after_the_preview_and_make_the_program() {
        let remote =
            with_make("#!/bin/sh\nmkdir -p bin\nprintf '#!/bin/sh\\nexec sleep 60\\n' > bin/run\nchmod +x bin/run\n");
        let home = TempDir::new();
        let places = places(&home);
        let fetched = fetch(&remote.origin(None), &places).expect("fetch");
        assert!(!fetched.checkout().join("bin/run").exists(), "nothing ran before the confirmation");
        assert!(fetched.preview().join("\n").contains("Installing it runs, in its folder:\n  ./make"));
        install(fetched, &places).expect("install");
        assert!(root(&places).join("bin/run").exists());
    }

    #[test]
    fn a_hidden_character_in_a_build_step_is_refused_before_the_preview() {
        let remote = Remote::new(&manifest("1.0", "\n[[build]]\ncommand = [\"./make\", \"a\u{202e}b\"]\n"));
        let home = TempDir::new();
        let places = places(&home);
        let error = fetch(&remote.origin(None), &places).map(|_| ()).expect_err("refused");
        assert!(error.contains("a\\u{202e}b") && !error.contains('\u{202e}'), "{error}");
        assert_eq!(staged(&places), 0);
    }

    #[test]
    fn a_build_that_changes_the_manifest_is_refused() {
        let remote = with_make(
            "#!/bin/sh\nprintf 'id = \"acme.notes\"\\nname = \"Notes\"\\nversion = \"9\"\\n' > cornercase-plugin.toml\n",
        );
        let error = refused(&remote);
        assert!(error.contains("changed cornercase-plugin.toml"), "{error}");
    }

    #[test]
    fn a_failing_build_installs_nothing() {
        let error = refused(&with_make("#!/bin/sh\nexit 3\n"));
        assert!(error.contains("`./make` failed"), "{error}");
    }
}

mod downloading {
    use super::*;

    fn archive() -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let mut header = tar::Header::new_gnu();
        header.set_size(SCRIPT.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, "bin/tool", SCRIPT.as_bytes()).expect("append");
        let mut gz = builder.into_inner().expect("tar");
        gz.flush().expect("flush");
        gz.finish().expect("gzip")
    }

    fn with_download(sha: &str, server: &FakeHttp) -> Option<Remote> {
        let target = crate::update::target()?;
        let extra = format!(
            "\n[[actions]]\nid = \"tool\"\ntitle = \"tool\"\ncontexts = [\"pane\"]\ncommand = [\"bin/tool\"]\n\n\
             [download.{target}]\nurl = \"{}/tool.tar.gz\"\nsha256 = \"{sha}\"\n",
            server.url()
        );
        Some(Remote::new(&manifest("1.0", &extra)))
    }

    #[test]
    fn a_download_that_matches_its_sha256_is_unpacked() {
        let bytes = archive();
        let server = FakeHttp::start(vec![("GET /tool.tar.gz", 200, bytes.clone())]);
        let Some(remote) = with_download(&hex(&Sha256::digest(&bytes)), &server) else { return };
        let home = TempDir::new();
        let places = places(&home);
        let fetched = fetch(&remote.origin(None), &places).expect("fetch");
        assert!(fetched.preview().join("\n").contains("/tool.tar.gz and checks its sha256"));
        install(fetched, &places).expect("install");
        let mode = fs::metadata(root(&places).join("bin/tool")).expect("unpacked").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[test]
    fn a_download_with_another_sha256_is_refused_before_unpacking() {
        let server = FakeHttp::start(vec![("GET /tool.tar.gz", 200, archive())]);
        let Some(remote) = with_download(&"0".repeat(64), &server) else { return };
        let error = refused(&remote);
        assert!(error.contains("does not match its sha256"), "{error}");
    }
}

mod updating {
    use super::*;

    pub(super) fn installed(remote: &Remote, places: &Places) {
        install(fetch(&remote.origin(None), places).expect("fetch"), places).expect("install");
    }

    #[test]
    fn the_same_commit_is_up_to_date() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        installed(&remote, &places);
        assert!(matches!(check_update(&places, "acme.notes"), Ok(Update::UpToDate(c)) if c == remote.head()));
    }

    #[test]
    fn a_new_commit_is_shown_again_and_keeps_the_plugin_on() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        installed(&remote, &places);
        let first = root(&places);
        let mut registry = Registry::open(&places.registry).expect("open");
        registry.set_enabled("acme.notes", true).expect("on");
        registry.save().expect("save");
        remote.commit(&[("cornercase-plugin.toml", &manifest("2.0", ""))]);
        let Ok(Update::Changed(fetched)) = check_update(&places, "acme.notes") else { panic!("an update") };
        let preview = fetched.preview();
        assert!(preview[0].contains("Notes 2.0"));
        assert!(preview.join("\n").contains("which is on: its new commands run right away"), "{preview:?}");
        let done = install(*fetched, &places).expect("update");
        let entry = Registry::load(&places.registry).entries[0].clone();
        assert_eq!(
            (done.enabled, entry.enabled, entry.source.root().ends_with(&remote.head()[..12]), first.exists()),
            (true, true, true, true)
        );
    }

    #[test]
    fn a_linked_plugin_is_not_updated() {
        let (home, folder) = (TempDir::new(), TempDir::new());
        let places = places(&home);
        linked(&places, &folder);
        let error = check_update(&places, "acme.notes").expect_err("refused");
        assert!(error.contains("linked plugin is read from its folder"), "{error}");
    }
}

mod removing {
    use super::*;

    #[test]
    fn an_installed_plugin_is_forgotten_and_its_copy_deleted() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        assert_eq!(remove(&places, "acme.notes"), Ok(Removed::Deleted));
        assert!(!places.installs.join("acme.notes").exists());
        assert_eq!(Registry::load(&places.registry).entries.len(), 0);
    }

    fn turned_on(places: &Places) {
        let mut registry = Registry::open(&places.registry).expect("open");
        registry.set_enabled("acme.notes", true).expect("on");
        registry.save().expect("save");
    }

    fn mark_running(places: &Places, pid: i32) {
        let dir = places.state("acme.notes").join("running");
        fs::create_dir_all(&dir).expect("running folder");
        fs::write(dir.join(pid.to_string()), "service sync").expect("running file");
    }

    #[test]
    fn the_service_has_stopped_when_remove_returns() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        turned_on(&places);
        let supervisor = Supervisor::start(Kind::Service, places.clone(), Vec::new());
        wait_until("the service runs", || supervise::running(&places, "acme.notes").is_ok_and(|pids| !pids.is_empty()));
        let pid = supervise::running(&places, "acme.notes").expect("markers")[0];
        assert_eq!(remove(&places, "acme.notes"), Ok(Removed::Deleted));
        assert!(!process::alive(pid));
        assert!(!places.installs.join("acme.notes").exists());
        drop(supervisor);
    }

    #[test]
    fn a_process_that_keeps_running_stops_the_removal() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        turned_on(&places);
        let sleeper = Sleeper::with_env(&[]);
        mark_running(&places, sleeper.pid());
        let err = remove_within(&places, "acme.notes", Duration::from_millis(300)).expect_err("still running");
        assert!(err.contains(&sleeper.pid().to_string()) && err.contains("remove acme.notes"), "{err}");
        let entry = Registry::load(&places.registry).entries[0].clone();
        assert_eq!((entry.enabled, entry.source.root().exists()), (false, true));
    }

    #[test]
    fn a_stale_pid_file_does_not_block() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        let pid = exited_pid();
        mark_running(&places, pid);
        let stale = places.state("acme.notes").join("running").join(pid.to_string());
        assert_eq!(remove_within(&places, "acme.notes", Duration::from_millis(300)), Ok(Removed::Deleted));
        assert!(!places.installs.join("acme.notes").exists());
        assert!(!stale.exists());
    }

    type Spawning = (std::thread::JoinHandle<()>, Arc<AtomicBool>);

    fn mid_spawn(places: &Places, work: impl FnOnce(&Places) + Send + 'static) -> Spawning {
        let lock = supervise::try_shared(places, "acme.notes").expect("lock").expect("free");
        let places = places.clone();
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            work(&places);
            std::thread::sleep(Duration::from_millis(150));
            flag.store(true, Ordering::SeqCst);
            drop(lock);
        });
        (handle, done)
    }

    #[test]
    fn a_spawn_in_progress_delays_the_removal_until_it_is_done() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        turned_on(&places);
        let (spawning, released) = mid_spawn(&places, |_| {});
        let result = remove_within(&places, "acme.notes", Duration::from_secs(5));
        assert!(released.load(Ordering::SeqCst), "remove returned while a spawn held the lock");
        spawning.join().expect("spawn");
        assert_eq!(result, Ok(Removed::Deleted));
    }

    #[test]
    fn a_process_started_by_that_spawn_stops_the_removal() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        turned_on(&places);
        let sleeper = Sleeper::with_env(&[]);
        let pid = sleeper.pid();
        let (spawning, _) = mid_spawn(&places, move |places| mark_running(places, pid));
        let result = remove_within(&places, "acme.notes", Duration::from_millis(600));
        spawning.join().expect("spawn");
        assert!(result.is_err_and(|e| e.contains(&pid.to_string())));
        assert!(places.installs.join("acme.notes").exists());
    }

    #[test]
    fn a_spawn_that_outlasts_the_wait_stops_the_removal() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        turned_on(&places);
        let (spawning, _) = mid_spawn(&places, |_| {});
        let err = remove_within(&places, "acme.notes", Duration::from_millis(100)).expect_err("still starting");
        spawning.join().expect("spawn");
        assert!(err.contains("still starting") && err.contains("nothing was deleted"), "{err}");
        let entry = Registry::load(&places.registry).entries[0].clone();
        assert_eq!((entry.enabled, entry.source.root().exists()), (false, true));
    }

    #[test]
    fn markers_that_cannot_be_read_stop_the_removal() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        let state = places.state("acme.notes");
        fs::create_dir_all(&state).expect("state folder");
        fs::write(state.join("running"), "not a folder").expect("running as a file");
        let err = remove_within(&places, "acme.notes", Duration::from_millis(300)).expect_err("unreadable");
        assert!(err.contains("cannot tell whether") && err.contains("nothing was deleted"), "{err}");
        assert!(places.installs.join("acme.notes").exists());
        assert_eq!(Registry::load(&places.registry).entries.len(), 1);
    }

    #[test]
    fn a_plugin_turned_on_again_during_the_wait_is_not_removed() {
        let (remote, home) = (Remote::new(&manifest("1.0", "")), TempDir::new());
        let places = places(&home);
        updating::installed(&remote, &places);
        let (again, _) = mid_spawn(&places, turned_on);
        let err = remove_within(&places, "acme.notes", Duration::from_secs(5)).expect_err("changed");
        again.join().expect("turned on");
        assert!(err.contains("changed while it was being removed"), "{err}");
        assert!(places.installs.join("acme.notes").exists());
        assert!(Registry::load(&places.registry).entries[0].enabled);
    }

    #[test]
    fn a_linked_plugin_keeps_its_folder() {
        let (home, folder) = (TempDir::new(), TempDir::new());
        let places = places(&home);
        linked(&places, &folder);
        assert_eq!(remove(&places, "acme.notes"), Ok(Removed::Unlinked));
        assert!(folder.path().join(manifest::FILE).exists());
    }
}

mod subfolders {
    use super::*;

    fn named(id: &str) -> String {
        manifest("1.0", "").replace("acme.notes", id)
    }

    fn shelf() -> Remote {
        let remote = Remote::new(&manifest("1.0", ""));
        for (folder, id) in [("plugins/one", "acme.one"), ("plugins/two", "acme.two")] {
            let dir = remote.work.path().join(folder);
            fs::create_dir_all(dir.join("bin")).expect("folder");
            fs::write(dir.join(manifest::FILE), named(id)).expect("manifest");
            write_executable(&dir.join("bin/run"), SCRIPT);
        }
        git(remote.work.path(), &["add", "--all"]);
        remote.commit(&[]);
        remote
    }

    fn from(remote: &Remote, subdir: &str) -> Origin {
        Origin { subdir: Some(subdir.into()), ..remote.origin(None) }
    }

    #[test]
    fn each_plugin_of_a_repository_installs_from_its_folder() {
        let (remote, home) = (shelf(), TempDir::new());
        let places = places(&home);
        let fetched = fetch(&from(&remote, "plugins/two"), &places).expect("fetch");
        assert!(fetched.preview().join("\n").contains("(folder plugins/two) at commit"));
        assert_eq!(install(fetched, &places).expect("install").manifest.id, "acme.two");
        install(fetch(&from(&remote, "plugins/one"), &places).expect("fetch"), &places).expect("install");
        let registry = Registry::load(&places.registry);
        let two = registry.entry("acme.two").expect("two").source.root().to_path_buf();
        assert_eq!((two.join("bin/run").exists(), two.join("plugins").exists()), (true, false));
        assert!(matches!(check_update(&places, "acme.two"), Ok(Update::UpToDate(_))));
        let Source::Git { subdir, .. } = &registry.entry("acme.one").expect("one").source else { panic!("git") };
        assert_eq!(subdir.as_deref(), Some("plugins/one"));
    }

    #[test]
    fn a_program_cannot_reach_out_of_its_folder_into_the_repository() {
        let remote = shelf();
        let run = remote.work.path().join("plugins/two/bin/run");
        fs::remove_file(&run).expect("remove");
        std::os::unix::fs::symlink("../../../bin/run", &run).expect("symlink");
        git(remote.work.path(), &["add", "--all"]);
        remote.commit(&[]);
        let home = TempDir::new();
        let places = places(&home);
        let fetched = fetch(&from(&remote, "plugins/two"), &places).expect("fetch");
        let error = install(fetched, &places).expect_err("refused");
        assert!(error.contains("outside the plugin's folder"), "{error}");
    }

    #[test]
    fn a_missing_folder_is_refused() {
        let (remote, home) = (shelf(), TempDir::new());
        let error = fetch(&from(&remote, "plugins/three"), &places(&home)).expect_err("refused");
        assert!(error.contains("has no folder `plugins/three`"), "{error}");
    }
}
