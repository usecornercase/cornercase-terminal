use super::*;
use crate::test_util::TempDir;

struct Homes {
    _dir: TempDir,
    default: PathBuf,
    account: PathBuf,
}

fn homes() -> Homes {
    let dir = TempDir::new();
    let (default, account) = (dir.path().join("claude"), dir.path().join("work"));
    fs::create_dir_all(default.join("skills")).expect("the default home");
    fs::create_dir(&account).expect("the account");
    fs::write(default.join("settings.json"), "{}").expect("settings");
    fs::write(default.join("CLAUDE.md"), "be brief").expect("memory");
    fs::write(default.join("skills").join("review.md"), "review").expect("a skill");
    fs::write(default.join("history.jsonl"), "").expect("not shared");
    Homes { _dir: dir, default, account }
}

#[test]
fn an_account_sees_the_shared_files_and_folders_of_the_default_home() {
    let homes = homes();
    assert_eq!(ensure(Known::Claude, &homes.default, &homes.account).expect("linked"), 3);
    fs::write(homes.default.join("settings.json"), r#"{"model":"opus"}"#).expect("a change");
    fs::write(homes.default.join("skills").join("test.md"), "test").expect("a new skill");
    let read = |path: &str| fs::read_to_string(homes.account.join(path)).expect("read through the link");
    assert_eq!((read("settings.json"), read("CLAUDE.md")), (r#"{"model":"opus"}"#.to_owned(), "be brief".to_owned()));
    assert_eq!(read("skills/test.md"), "test");
    assert!(!homes.account.join("history.jsonl").exists());
    assert_eq!(status(Known::Claude, &homes.default, &homes.account), Vec::<String>::new());
}

#[test]
fn a_file_put_in_place_of_a_link_is_kept_and_reported() {
    let homes = homes();
    ensure(Known::Claude, &homes.default, &homes.account).expect("linked");
    fs::remove_file(homes.account.join("settings.json")).expect("remove the link");
    fs::write(homes.account.join("settings.json"), "mine").expect("an agent's own file");
    assert_eq!(ensure(Known::Claude, &homes.default, &homes.account).expect("ensured"), 0);
    assert_eq!(fs::read_to_string(homes.account.join("settings.json")).expect("read"), "mine");
    assert_eq!(status(Known::Claude, &homes.default, &homes.account), ["settings.json"]);
}
