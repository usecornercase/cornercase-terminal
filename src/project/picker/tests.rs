use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, git_repo};

fn tree(folders: &[&str]) -> TempDir {
    let tmp = TempDir::new();
    for folder in folders {
        std::fs::create_dir_all(tmp.path().join(folder)).expect("create folder");
    }
    tmp
}

fn open(dir: &Path) -> Picker {
    Picker::open(dir, None).expect("open picker")
}

fn names(picker: &Picker) -> Vec<&str> {
    picker.items().into_iter().map(|item| item.name.as_str()).collect()
}

fn type_text(picker: &mut Picker, text: &str) {
    for c in text.chars() {
        picker.push(c);
    }
}

mod listing {
    use super::*;

    #[test]
    fn shows_the_parent_then_the_folders_sorted() {
        let tmp = tree(&["beta", "Alpha", "gamma"]);
        assert_eq!(names(&open(tmp.path())), ["..", "Alpha", "beta", "gamma"]);
    }

    #[test]
    fn leaves_out_files() {
        let tmp = tree(&["src"]);
        std::fs::write(tmp.path().join("README"), "hi").expect("write file");
        assert_eq!(names(&open(tmp.path())), ["..", "src"]);
    }

    #[test]
    fn hides_hidden_folders() {
        let tmp = tree(&[".git", "src"]);
        assert_eq!(names(&open(tmp.path())), ["..", "src"]);
    }

    #[test]
    fn lists_vscode_workspaces_after_the_folders() {
        let tmp = tree(&["src", "z"]);
        for file in ["a.code-workspace", "notes.json"] {
            std::fs::write(tmp.path().join(file), "{}").expect("write file");
        }
        assert_eq!(names(&open(tmp.path())), ["..", "src", "z", "a.code-workspace"]);
    }

    #[test]
    fn root_has_no_parent() {
        assert!(open(Path::new("/")).items().iter().all(|item| item.name != ".."));
    }

    #[test]
    fn shows_the_branch_of_repos() {
        let tmp = TempDir::new();
        let repo = git_repo(&[]);
        let link = tmp.path().join("repo");
        std::fs::rename(repo.path(), &link).expect("move repo");
        std::fs::create_dir(tmp.path().join("plain")).expect("create folder");

        let picker = open(tmp.path());

        let branches: Vec<_> = picker.items().iter().map(|item| item.branch.as_deref()).collect();
        assert_eq!(branches, [None, None, Some("main")]);
    }

    #[test]
    fn subfolders_of_a_repo_show_no_branch() {
        let repo = git_repo(&[("src/main.rs", "")]);
        assert!(open(repo.path()).items().iter().all(|item| item.branch.is_none()));
    }
}

mod workspaces {
    use super::*;

    fn with_workspace() -> (TempDir, Picker) {
        let tmp = tree(&["src"]);
        std::fs::write(tmp.path().join("w.code-workspace"), "{}").expect("write file");
        let picker = open(tmp.path());
        (tmp, picker)
    }

    #[test]
    fn choosing_one_returns_it_and_stays() {
        let (tmp, mut picker) = with_workspace();

        let chosen = picker.choose(2);

        assert_eq!((chosen, picker.dir()), (Some(tmp.path().join("w.code-workspace")), tmp.path()));
    }

    #[test]
    fn choosing_a_folder_goes_into_it() {
        let (tmp, mut picker) = with_workspace();

        let chosen = picker.choose(1);

        assert_eq!((chosen, picker.dir()), (None, tmp.path().join("src").as_path()));
    }

    #[test]
    fn going_into_one_does_nothing() {
        let (tmp, mut picker) = with_workspace();

        picker.enter(2);

        assert_eq!(picker.dir(), tmp.path());
    }

    #[test]
    fn filtering_keeps_them_after_the_folders() {
        let tmp = tree(&["data"]);
        std::fs::write(tmp.path().join("a.code-workspace"), "{}").expect("write file");
        let mut picker = open(tmp.path());

        picker.push('a');

        assert_eq!(names(&picker), ["data", "a.code-workspace"]);
    }

    #[test]
    fn submitting_the_selected_one_returns_it() {
        let (tmp, mut picker) = with_workspace();
        type_text(&mut picker, "w.");

        assert_eq!(picker.submit(), Some(tmp.path().join("w.code-workspace")));
    }
}

mod filter {
    use super::*;

    #[rstest]
    #[case::exact_and_prefix_first("web", &["aweb", "web-old", "web"], &["web", "web-old", "aweb"])]
    #[case::case_insensitive("SRC", &["src", "other"], &["src"])]
    #[case::hidden_with_a_dot(".c", &[".config", "c"], &[".config"])]
    #[case::nothing_matches("zzz", &["src"], &[])]
    fn ranks_exact_then_prefix_then_substring(
        #[case] filter: &str,
        #[case] folders: &[&str],
        #[case] expected: &[&str],
    ) {
        let tmp = tree(folders);
        let mut picker = open(tmp.path());

        type_text(&mut picker, filter);

        assert_eq!(names(&picker), expected);
    }

    #[test]
    fn typing_selects_the_first_match() {
        let tmp = tree(&["src"]);
        let mut picker = open(tmp.path());

        picker.push('s');

        assert_eq!(picker.selected(), Some(0));
    }

    #[test]
    fn backspace_deletes_from_the_filter() {
        let tmp = tree(&["src"]);
        let mut picker = open(tmp.path());
        type_text(&mut picker, "sr");

        picker.pop();

        assert_eq!((picker.filter(), picker.dir()), ("s", tmp.path()));
    }

    #[test]
    fn clearing_the_filter_clears_the_selection() {
        let tmp = tree(&["src"]);
        let mut picker = open(tmp.path());
        picker.push('s');

        picker.pop();

        assert_eq!(picker.selected(), None);
    }
}

mod navigation {
    use super::*;

    #[test]
    fn enter_goes_into_a_folder() {
        let tmp = tree(&["src/inner"]);
        let mut picker = open(tmp.path());

        picker.enter(1);

        assert_eq!((picker.dir(), names(&picker)), (tmp.path().join("src").as_path(), vec!["..", "inner"]));
    }

    #[test]
    fn the_parent_item_goes_up() {
        let tmp = tree(&["src"]);
        let mut picker = open(&tmp.path().join("src"));

        picker.enter(0);

        assert_eq!(picker.dir(), tmp.path());
    }

    #[test]
    fn backspace_with_no_filter_goes_up() {
        let tmp = tree(&["src"]);
        let mut picker = open(&tmp.path().join("src"));

        picker.pop();

        assert_eq!(picker.dir(), tmp.path());
    }

    #[test]
    fn typing_a_path_walks_it() {
        let tmp = tree(&["projects/cornercase", "projects/cornercase-web"]);
        let mut picker = open(Path::new("/"));

        type_text(&mut picker, &format!("{}/projects/cornercase/", tmp.path().display()));

        assert_eq!(picker.dir(), tmp.path().join("projects/cornercase"));
    }

    #[test]
    fn tilde_goes_home() {
        let (home, other) = (TempDir::new(), TempDir::new());
        let mut picker = Picker::open(other.path(), Some(home.path())).expect("open picker");

        picker.push('~');

        assert_eq!(picker.dir(), home.path());
    }

    #[test]
    fn an_unreadable_folder_keeps_the_picker_where_it_was() {
        let tmp = tree(&["src"]);
        let mut picker = open(tmp.path());
        std::fs::remove_dir(tmp.path().join("src")).expect("remove folder");

        picker.enter(1);

        assert_eq!((picker.dir(), picker.error().is_some()), (tmp.path(), true));
    }
}

mod submit {
    use super::*;

    #[test]
    fn opens_the_current_folder_when_nothing_is_selected() {
        let tmp = tree(&["src"]);
        assert_eq!(open(tmp.path()).submit(), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn goes_into_the_selected_folder() {
        let tmp = tree(&["src"]);
        let mut picker = open(tmp.path());
        picker.push('s');

        let opened = picker.submit();

        assert_eq!((opened, picker.dir()), (None, tmp.path().join("src").as_path()));
    }

    #[test]
    fn does_nothing_when_the_filter_matches_nothing() {
        let tmp = tree(&["src"]);
        let mut picker = open(tmp.path());
        picker.push('z');

        assert_eq!((picker.submit(), picker.dir()), (None, tmp.path()));
    }
}

mod scrolling {
    use super::*;

    fn many() -> TempDir {
        tree(&["a", "b", "c", "d", "e", "f"])
    }

    #[test]
    fn moving_down_from_nothing_selects_the_first() {
        let tmp = many();
        let mut picker = open(tmp.path());

        picker.move_selection(1, 3);

        assert_eq!(picker.selected(), Some(0));
    }

    #[test]
    fn moving_up_from_nothing_selects_the_last() {
        let tmp = many();
        let mut picker = open(tmp.path());

        picker.move_selection(-1, 3);

        assert_eq!((picker.selected(), picker.scroll()), (Some(6), 4));
    }

    #[test]
    fn moving_past_the_last_visible_row_scrolls() {
        let tmp = many();
        let mut picker = open(tmp.path());
        for _ in 0..4 {
            picker.move_selection(1, 3);
        }

        assert_eq!((picker.selected(), picker.scroll()), (Some(3), 1));
    }

    #[test]
    fn the_wheel_stops_at_the_end() {
        let tmp = many();
        let mut picker = open(tmp.path());

        picker.scroll_by(100, 3);

        assert_eq!(picker.scroll(), 4);
    }

    #[test]
    fn the_wheel_stops_at_the_top() {
        let tmp = many();
        let mut picker = open(tmp.path());

        picker.scroll_by(-3, 3);

        assert_eq!(picker.scroll(), 0);
    }
}
