use std::cell::Cell;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::test_util::TempDir;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, text).expect("write");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("read")
}

fn inode(path: &Path) -> u64 {
    fs::metadata(path).expect("metadata").ino()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| entry.expect("entry").file_name().into_string().expect("utf-8"))
        .collect();
    names.sort();
    names
}

struct NoLinks {
    tried: Cell<usize>,
}

impl NoLinks {
    fn new() -> Self {
        Self { tried: Cell::new(0) }
    }
}

impl Linker for NoLinks {
    fn link(&self, _: &Path, _: &Path) -> io::Result<()> {
        self.tried.set(self.tried.get() + 1);
        Err(io::Error::new(io::ErrorKind::CrossesDevices, "another disk"))
    }
}

struct BadCopy;

impl Linker for BadCopy {
    fn link(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::CrossesDevices, "another disk"))
    }

    fn copy(&self, _: &Path, to: &mut File) -> io::Result<u64> {
        to.write_all(b"not the transcript")?;
        Ok(18)
    }
}

struct Setup {
    _dir: TempDir,
    old: PathBuf,
    new: PathBuf,
}

impl Setup {
    fn new() -> Self {
        let dir = TempDir::new();
        let old = dir.path().join("old");
        let new = dir.path().join("new");
        fs::create_dir_all(&old).expect("old home");
        fs::create_dir_all(&new).expect("new home");
        Self { _dir: dir, old, new }
    }

    fn transcript(&self) -> (PathBuf, PathBuf) {
        let source = self.old.join("projects/-code-web/abc.jsonl");
        write(&source, "{\"type\":\"user\"}\n");
        (source, PathBuf::from("projects/-code-web/abc.jsonl"))
    }
}

mod linking {
    use super::*;

    #[test]
    fn a_missing_file_is_linked_to_the_same_inode_and_the_source_stays() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let tally = conversation(&setup.new, &[(source.clone(), relative.clone())], &Disk).expect("moved");
        assert_eq!(tally, Tally { linked: 1, ..Tally::default() });
        let dest = setup.new.join(&relative);
        assert_eq!(inode(&dest), inode(&source));
        assert_eq!(read(&source), "{\"type\":\"user\"}\n");
    }

    #[test]
    fn new_folders_are_private() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        conversation(&setup.new, &[(source, relative)], &Disk).expect("moved");
        assert_eq!(mode(&setup.new.join("projects")), 0o700);
        assert_eq!(mode(&setup.new.join("projects/-code-web")), 0o700);
    }

    #[test]
    fn an_existing_folder_keeps_its_mode() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        fs::create_dir_all(setup.new.join("projects")).expect("projects");
        fs::set_permissions(setup.new.join("projects"), fs::Permissions::from_mode(0o755)).expect("chmod");
        conversation(&setup.new, &[(source, relative)], &Disk).expect("moved");
        assert_eq!(mode(&setup.new.join("projects")), 0o755);
    }

    #[test]
    fn the_same_file_already_there_is_present() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        conversation(&setup.new, &[(source.clone(), relative.clone())], &Disk).expect("first");
        let tally = conversation(&setup.new, &[(source, relative)], &Disk).expect("again");
        assert_eq!(tally, Tally { present: 1, ..Tally::default() });
    }

    #[test]
    fn an_equal_copy_already_there_is_present_and_untouched() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let dest = setup.new.join(&relative);
        write(&dest, &read(&source));
        let before = inode(&dest);
        let tally = conversation(&setup.new, &[(source, relative)], &Disk).expect("present");
        assert_eq!(tally, Tally { present: 1, ..Tally::default() });
        assert_eq!(inode(&dest), before);
    }
}

mod copying {
    use super::*;

    #[test]
    fn a_failing_link_copies_and_checks_the_copy() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let linker = NoLinks::new();
        let tally = conversation(&setup.new, &[(source.clone(), relative.clone())], &linker).expect("copied");
        assert_eq!(tally, Tally { copied: 1, ..Tally::default() });
        assert_eq!(linker.tried.get(), 1);
        let dest = setup.new.join(&relative);
        assert_ne!(inode(&dest), inode(&source));
        assert_eq!(read(&dest), read(&source));
        assert_eq!(mode(&dest), 0o600);
        assert_eq!(entries(dest.parent().expect("parent")), ["abc.jsonl"]);
    }

    #[test]
    fn a_copy_that_does_not_match_is_removed_and_fails() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let error = conversation(&setup.new, &[(source.clone(), relative)], &BadCopy).expect_err("refused");
        assert!(error.to_string().contains("changed while it was copied"), "{error}");
        assert_eq!(entries(&setup.new.join("projects/-code-web")), Vec::<String>::new());
        assert_eq!(read(&source), "{\"type\":\"user\"}\n");
    }
}

mod refusing {
    use super::*;

    #[test]
    fn a_different_file_already_there_is_never_overwritten() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let dest = setup.new.join(&relative);
        write(&dest, "theirs");
        for linker in [&Disk as &dyn Linker, &NoLinks::new()] {
            let error = conversation(&setup.new, &[(source.clone(), relative.clone())], linker).expect_err("refused");
            assert!(error.to_string().contains("other contents"), "{error}");
            assert_eq!(read(&dest), "theirs");
        }
    }

    #[test]
    fn a_linked_folder_on_the_way_is_refused_and_says_so() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let elsewhere = setup.old.join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, setup.new.join("projects")).expect("symlink");
        let error = conversation(&setup.new, &[(source, relative)], &Disk).expect_err("refused");
        assert!(error.to_string().contains("does not write through links"), "{error}");
        assert_eq!(entries(&elsewhere), Vec::<String>::new());
    }

    #[test]
    fn a_folder_where_the_file_goes_is_refused() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        fs::create_dir_all(setup.new.join(&relative)).expect("folder");
        let error = conversation(&setup.new, &[(source, relative)], &Disk).expect_err("refused");
        assert!(error.to_string().contains("not a file"), "{error}");
    }

    #[rstest::rstest]
    #[case::parent("../escape.jsonl")]
    #[case::absolute("/tmp/escape.jsonl")]
    #[case::empty("")]
    #[case::dot("./projects/abc.jsonl")]
    fn a_path_outside_the_account_is_refused(#[case] relative: &str) {
        let setup = Setup::new();
        let (source, _) = setup.transcript();
        let error = conversation(&setup.new, &[(source, PathBuf::from(relative))], &Disk).expect_err("refused");
        assert!(error.to_string().contains("not a path inside the account"), "{error}");
        assert_eq!(entries(&setup.new), Vec::<String>::new());
    }

    #[test]
    fn a_link_as_the_source_is_refused() {
        let setup = Setup::new();
        let (source, _) = setup.transcript();
        let link = setup.old.join("link.jsonl");
        std::os::unix::fs::symlink(&source, &link).expect("symlink");
        let error = conversation(&setup.new, &[(link, PathBuf::from("link.jsonl"))], &Disk).expect_err("refused");
        assert!(error.to_string().contains("not a file or a folder"), "{error}");
        assert_eq!(entries(&setup.new), Vec::<String>::new());
    }

    #[test]
    fn a_missing_source_is_refused() {
        let setup = Setup::new();
        let missing = setup.old.join("gone.jsonl");
        let error = conversation(&setup.new, &[(missing, PathBuf::from("gone.jsonl"))], &Disk).expect_err("refused");
        assert!(error.to_string().contains("could not read"), "{error}");
    }
}

mod folders {
    use super::*;

    fn subagents(setup: &Setup) -> PathBuf {
        let folder = setup.old.join("projects/-code-web/abc");
        write(&folder.join("subagents/agent-1.jsonl"), "one\n");
        write(&folder.join("subagents/agent-1.meta.json"), "{}");
        write(&folder.join("tool-results/out.txt"), "output");
        folder
    }

    #[test]
    fn are_brought_whole_with_every_file_linked() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let folder = subagents(&setup);
        let files = [(source, relative), (folder.clone(), PathBuf::from("projects/-code-web/abc"))];
        let tally = conversation(&setup.new, &files, &Disk).expect("moved");
        assert_eq!(tally, Tally { linked: 4, ..Tally::default() });
        let dest = setup.new.join("projects/-code-web/abc");
        assert_eq!(entries(&dest), ["subagents", "tool-results"]);
        assert_eq!(entries(&dest.join("subagents")), ["agent-1.jsonl", "agent-1.meta.json"]);
        assert_eq!(inode(&dest.join("subagents/agent-1.jsonl")), inode(&folder.join("subagents/agent-1.jsonl")));
        assert_eq!(mode(&dest), 0o700);
        assert_eq!(mode(&dest.join("subagents")), 0o700);
    }

    #[test]
    fn are_copied_when_links_fail() {
        let setup = Setup::new();
        let folder = subagents(&setup);
        let tally = conversation(&setup.new, &[(folder, PathBuf::from("abc"))], &NoLinks::new()).expect("copied");
        assert_eq!(tally, Tally { copied: 3, ..Tally::default() });
        assert_eq!(read(&setup.new.join("abc/subagents/agent-1.jsonl")), "one\n");
    }

    #[test]
    fn links_inside_are_left_out_and_never_followed() {
        let setup = Setup::new();
        let folder = subagents(&setup);
        let secret = setup.old.join("secret");
        write(&secret.join("key"), "token");
        std::os::unix::fs::symlink(&secret, folder.join("outside")).expect("symlink folder");
        std::os::unix::fs::symlink(secret.join("key"), folder.join("key")).expect("symlink file");
        let tally = conversation(&setup.new, &[(folder, PathBuf::from("abc"))], &Disk).expect("moved");
        assert_eq!(tally.linked, 3);
        assert_eq!(entries(&setup.new.join("abc")), ["subagents", "tool-results"]);
    }

    #[test]
    fn a_different_file_inside_stops_and_keeps_theirs() {
        let setup = Setup::new();
        let folder = subagents(&setup);
        write(&setup.new.join("abc/subagents/agent-1.jsonl"), "theirs\n");
        let error = conversation(&setup.new, &[(folder, PathBuf::from("abc"))], &Disk).expect_err("refused");
        assert!(error.to_string().contains("other contents"), "{error}");
        assert_eq!(read(&setup.new.join("abc/subagents/agent-1.jsonl")), "theirs\n");
    }

    #[test]
    fn a_conflict_anywhere_links_nothing() {
        let setup = Setup::new();
        let (source, relative) = setup.transcript();
        let folder = subagents(&setup);
        write(&setup.new.join("projects/-code-web/abc/tool-results/out.txt"), "theirs");
        let files = [(source, relative.clone()), (folder, PathBuf::from("projects/-code-web/abc"))];
        let error = conversation(&setup.new, &files, &Disk).expect_err("refused");
        assert!(error.to_string().contains("other contents"), "{error}");
        assert!(!setup.new.join(&relative).exists());
        assert!(!setup.new.join("projects/-code-web/abc/subagents").exists());
    }

    #[test]
    fn the_source_is_never_changed() {
        let setup = Setup::new();
        let folder = subagents(&setup);
        let before = entries(&folder.join("subagents"));
        conversation(&setup.new, &[(folder.clone(), PathBuf::from("abc"))], &NoLinks::new()).expect("copied");
        assert_eq!(entries(&folder.join("subagents")), before);
        assert_eq!(read(&folder.join("subagents/agent-1.jsonl")), "one\n");
    }
}
