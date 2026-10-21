use rstest::rstest;

use super::*;
use crate::changes::diff::{Fold, Status};

fn file(path: &str, old_path: Option<&str>, status: Status) -> File {
    File {
        path: path.into(),
        old_path: old_path.map(str::to_string),
        status,
        added: 0,
        removed: 0,
        fold: Fold::Open,
        hunks: Vec::new(),
        digest: 0,
        viewed_digest: 0,
        staged: None,
    }
}

fn record(kind: &str, xy: &str, path: &str) -> String {
    let tail = match kind {
        "1" => "N... 100644 100644 100644 aaaa bbbb",
        "2" => "N... 100644 100644 100644 aaaa bbbb R100",
        _ => "N... 100644 100644 100644 100644 aaaa bbbb cccc",
    };
    format!("{kind} {xy} {tail} {path}\0")
}

mod sides {
    use super::*;

    #[rstest]
    #[case::unstaged(".M", Staged::No)]
    #[case::staged("M.", Staged::Yes)]
    #[case::partly("MM", Staged::Partly)]
    #[case::added("A.", Staged::Yes)]
    #[case::intent_to_add(".A", Staged::No)]
    #[case::deleted_in_the_index("D.", Staged::Yes)]
    #[case::deleted_in_the_tree(".D", Staged::No)]
    #[case::type_change("T.", Staged::Yes)]
    fn of_a_plain_record(#[case] xy: &str, #[case] want: Staged) {
        let porcelain = Porcelain::parse(record("1", xy, "a.txt").as_bytes(), "");
        assert_eq!(porcelain.staged(&file("a.txt", None, Status::Modified)), want);
    }

    #[rstest]
    #[case::staged("R.", Staged::Yes)]
    #[case::edited_after("RM", Staged::Partly)]
    #[case::intent_to_add(".R", Staged::No)]
    fn of_a_rename(#[case] xy: &str, #[case] want: Staged) {
        let raw = format!("{}old.txt\0", record("2", xy, "new.txt"));
        let porcelain = Porcelain::parse(raw.as_bytes(), "");
        assert_eq!(porcelain.staged(&file("new.txt", Some("old.txt"), Status::Renamed)), want);
        assert_eq!(porcelain.renames.get("new.txt").map(String::as_str), Some("old.txt"));
    }

    #[test]
    fn of_a_rename_the_index_sees_as_two_files() {
        let raw = format!("{}{}", record("1", "A.", "c.txt"), record("1", ".D", "a.txt"));
        let porcelain = Porcelain::parse(raw.as_bytes(), "");
        assert_eq!(porcelain.staged(&file("c.txt", Some("a.txt"), Status::Renamed)), Staged::Partly);
    }

    #[test]
    fn of_an_unmerged_file() {
        let porcelain = Porcelain::parse(record("u", "UU", "a.txt").as_bytes(), "");
        assert_eq!(porcelain.staged(&file("a.txt", None, Status::Modified)), Staged::No);
    }

    #[test]
    fn of_an_untracked_file_and_of_one_git_did_not_list() {
        let porcelain = Porcelain::parse(b"? b.txt\0", "");
        assert_eq!(porcelain.untracked, ["b.txt"]);
        assert_eq!(porcelain.staged(&file("b.txt", None, Status::Untracked)), Staged::No);
        assert_eq!(porcelain.staged(&file("zzz", None, Status::Modified)), Staged::No);
    }
}

mod prefix {
    use super::*;

    #[test]
    fn is_stripped_and_other_folders_are_skipped() {
        let raw =
            format!("{}{}? proj/new.txt\0? top.txt\0", record("1", "M.", "proj/p.txt"), record("1", ".M", "top.txt"));
        let porcelain = Porcelain::parse(raw.as_bytes(), "proj/");
        assert_eq!(porcelain.staged(&file("p.txt", None, Status::Modified)), Staged::Yes);
        assert_eq!(porcelain.staged(&file("top.txt", None, Status::Modified)), Staged::No);
        assert_eq!(porcelain.untracked, ["new.txt"]);
        assert_eq!(porcelain.statuses.len(), 1);
    }
}
