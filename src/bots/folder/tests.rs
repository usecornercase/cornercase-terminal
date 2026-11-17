use super::*;
use crate::agents::headless::Access;
use crate::bots::sample as bot;
use crate::test_util::TempDir;

#[test]
fn a_new_bot_gets_its_four_files() {
    let dir = TempDir::new();
    let folder = create(dir.path(), &bot("prs", "PRs"), "review my PRs").expect("create");
    assert_eq!(folder, dir.path().join("prs"));
    assert_eq!(std::fs::read_to_string(folder.join(ABOUT_FILE)).expect("bot.md"), "review my PRs");
    assert_eq!(std::fs::read_to_string(folder.join(MEMORY_FILE)).expect("memory.md"), "");
    assert_eq!(std::fs::read_to_string(folder.join(CHAT_FILE)).expect("chat.jsonl"), "");
    assert_eq!(load_all(dir.path()), vec![bot("prs", "PRs")]);
    assert_eq!(notes(&folder), (String::from("review my PRs"), String::new()));
}

#[test]
fn what_a_bot_is_for_reads_as_it_is_or_empty_when_missing_and_fails_when_unreadable() {
    let dir = TempDir::new();
    let folder = create(dir.path(), &bot("prs", "PRs"), "review my PRs\n").expect("create");
    assert_eq!(about(&folder).expect("read"), "review my PRs\n");
    std::fs::remove_file(folder.join(ABOUT_FILE)).expect("remove");
    assert_eq!(about(&folder).expect("missing"), "");
    std::fs::write(folder.join(ABOUT_FILE), [0xff, 0xfe]).expect("bytes");
    assert_eq!(about(&folder).expect_err("not utf-8").kind(), io::ErrorKind::InvalidData);
    std::fs::remove_file(folder.join(ABOUT_FILE)).expect("remove");
    std::fs::create_dir(folder.join(ABOUT_FILE)).expect("folder");
    assert_ne!(about(&folder).expect_err("a folder").kind(), io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn a_bot_md_that_is_a_fifo_is_refused_without_waiting() {
    let dir = TempDir::new();
    let folder = create(dir.path(), &bot("prs", "PRs"), "").expect("create");
    std::fs::remove_file(folder.join(ABOUT_FILE)).expect("remove");
    let made = std::process::Command::new("mkfifo").arg(folder.join(ABOUT_FILE)).status().expect("mkfifo");
    assert!(made.success());
    assert!(about(&folder).expect_err("refused").to_string().contains("not a regular file"));
}

#[test]
fn bots_load_by_when_they_were_made_and_odd_folders_are_skipped() {
    let dir = TempDir::new();
    let mut later = bot("later", "Later");
    later.created = 300;
    create(dir.path(), &later, "").expect("create");
    create(dir.path(), &bot("early", "Early"), "").expect("create");
    std::fs::create_dir_all(dir.path().join("empty")).expect("folder without bot.json");
    std::fs::write(dir.path().join("a-file"), "x").expect("file");
    create(dir.path(), &bot("moved", "Moved"), "").expect("create");
    std::fs::rename(dir.path().join("moved"), dir.path().join("elsewhere")).expect("rename");
    let ids: Vec<_> = load_all(dir.path()).into_iter().map(|bot| bot.id).collect();
    assert_eq!(ids, ["early", "later"]);
}

#[test]
fn a_saved_bot_keeps_its_session_and_a_removed_one_is_gone() {
    let dir = TempDir::new();
    let folder = create(dir.path(), &bot("prs", "PRs"), "").expect("create");
    let mut changed = bot("prs", "PRs");
    changed.conversation = Some(String::from("s1"));
    changed.access = Access::Full;
    save(&folder, &changed).expect("save");
    assert_eq!(load_all(dir.path()), vec![changed]);
    remove(&folder).expect("remove");
    assert_eq!(load_all(dir.path()), Vec::new());
}

#[cfg(unix)]
#[test]
fn the_folder_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let folder = create(dir.path(), &bot("prs", "PRs"), "x").expect("create");
    let mode = |path: &Path| std::fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(&folder), 0o700);
    for file in [BOT_FILE, ABOUT_FILE, MEMORY_FILE, CHAT_FILE] {
        assert_eq!(mode(&folder.join(file)), 0o600, "{file}");
    }
}
