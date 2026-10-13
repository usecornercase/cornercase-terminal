use rstest::rstest;

use super::*;
use crate::paths;
use crate::test_util::TempDir;

fn texts(todos: &Todos) -> Vec<String> {
    todos.items().iter().map(|i| if i.done { format!("[x] {}", i.text) } else { i.text.clone() }).collect()
}

fn with(texts: &[&str]) -> (Todos, Vec<u64>) {
    let mut todos = Todos::default();
    let ids = texts.iter().map(|t| todos.add(t).expect("added")).collect();
    (todos, ids)
}

mod adding {
    use super::*;

    #[test]
    fn items_go_to_the_end_of_the_list() {
        let (todos, _) = with(&["fix login", "write docs"]);
        assert_eq!(texts(&todos), ["fix login", "write docs"]);
    }

    #[test]
    fn a_new_item_goes_before_the_done_ones() {
        let (mut todos, ids) = with(&["fix login"]);
        todos.toggle(ids[0]);
        todos.add("write docs");
        assert_eq!(texts(&todos), ["write docs", "[x] fix login"]);
    }

    #[rstest]
    #[case::empty("")]
    #[case::blank("  \n ")]
    fn blank_text_adds_nothing(#[case] text: &str) {
        let mut todos = Todos::default();
        assert_eq!(todos.add(text), None);
        assert_eq!(todos, Todos::default());
    }

    #[test]
    fn text_is_trimmed_and_kept_on_one_line() {
        let (todos, _) = with(&["  fix\nlogin  "]);
        assert_eq!(texts(&todos), ["fix login"]);
    }
}

mod editing {
    use super::*;

    #[test]
    fn editing_replaces_the_text() {
        let (mut todos, ids) = with(&["fix logn"]);
        assert!(todos.edit(ids[0], "fix login"));
        assert_eq!(texts(&todos), ["fix login"]);
    }

    #[test]
    fn blank_text_keeps_the_old_one() {
        let (mut todos, ids) = with(&["fix login"]);
        assert!(!todos.edit(ids[0], " "));
        assert_eq!(texts(&todos), ["fix login"]);
    }
}

mod checking {
    use super::*;

    #[test]
    fn a_checked_item_goes_to_the_bottom() {
        let (mut todos, ids) = with(&["a", "b", "c"]);
        todos.toggle(ids[0]);
        todos.toggle(ids[1]);
        assert_eq!((texts(&todos), todos.pending()), (vec!["c".into(), "[x] a".into(), "[x] b".into()], 1));
    }

    #[test]
    fn an_unchecked_item_goes_to_the_end_of_the_pending_ones() {
        let (mut todos, ids) = with(&["a", "b", "c"]);
        todos.toggle(ids[0]);
        todos.toggle(ids[1]);
        todos.toggle(ids[1]);
        assert_eq!(texts(&todos), ["c", "b", "[x] a"]);
    }
}

mod reordering {
    use super::*;

    #[rstest]
    #[case::down(0, 3, &["b", "c", "a", "[x] d"])]
    #[case::up(2, 0, &["c", "a", "b", "[x] d"])]
    #[case::into_the_done_ones_stops_before_them(0, 4, &["b", "c", "a", "[x] d"])]
    fn a_pending_item_moves_among_the_pending_ones(
        #[case] from: usize,
        #[case] before: usize,
        #[case] expected: &[&str],
    ) {
        let (mut todos, ids) = with(&["a", "b", "c", "d"]);
        todos.toggle(ids[3]);
        todos.move_before(ids[from], before);
        assert_eq!(texts(&todos), expected);
    }

    #[test]
    fn a_done_item_stays_among_the_done_ones() {
        let (mut todos, ids) = with(&["a", "b", "c"]);
        todos.toggle(ids[1]);
        todos.toggle(ids[2]);
        todos.move_before(ids[2], 0);
        assert_eq!(texts(&todos), ["a", "[x] c", "[x] b"]);
    }
}

mod removing {
    use super::*;

    #[test]
    fn undo_puts_an_item_back_where_it_was() {
        let (mut todos, ids) = with(&["a", "b", "c"]);
        let removed = todos.remove(ids[1]).expect("removed");
        assert_eq!(texts(&todos), ["a", "c"]);
        todos.restore(removed);
        assert_eq!(texts(&todos), ["a", "b", "c"]);
    }

    #[test]
    fn clearing_removes_only_the_done_items() {
        let (mut todos, ids) = with(&["a", "b", "c"]);
        todos.toggle(ids[0]);
        todos.toggle(ids[2]);
        let removed = todos.clear_done().expect("cleared");
        assert_eq!((texts(&todos), removed.len()), (vec!["b".to_string()], 2));
    }

    #[test]
    fn clearing_without_done_items_removes_nothing() {
        let (mut todos, _) = with(&["a"]);
        assert_eq!(todos.clear_done(), None);
    }

    #[test]
    fn undo_puts_cleared_items_back_in_order() {
        let (mut todos, ids) = with(&["a", "b", "c"]);
        todos.toggle(ids[0]);
        todos.toggle(ids[1]);
        let removed = todos.clear_done().expect("cleared");
        todos.restore(removed);
        assert_eq!(texts(&todos), ["c", "[x] a", "[x] b"]);
    }

    #[test]
    fn undo_keeps_pending_items_before_done_ones() {
        let (mut todos, ids) = with(&["a", "b", "c"]);
        let removed = todos.remove(ids[2]).expect("removed");
        todos.toggle(ids[0]);
        todos.toggle(ids[1]);
        todos.restore(removed);
        assert_eq!(texts(&todos), ["c", "[x] a", "[x] b"]);
    }
}

mod saving {
    use super::*;

    #[test]
    fn the_list_comes_back_from_its_file() {
        let tmp = TempDir::new();
        let path = tmp.path().join(paths::TODOS_FILE);
        let (mut todos, ids) = with(&["a", "b"]);
        todos.toggle(ids[0]);
        crate::persist::save(&path, &todos.saved()).expect("save");
        assert_eq!(texts(&load(&path)), ["b", "[x] a"]);
    }

    #[test]
    fn the_file_holds_only_text_and_done() {
        let (mut todos, ids) = with(&["a", "b"]);
        todos.toggle(ids[0]);
        let json = serde_json::to_value(todos.saved()).expect("json");
        assert_eq!(json, serde_json::json!({"version": 1, "items": [{"text": "b"}, {"text": "a", "done": true}]}));
    }

    #[test]
    fn a_hand_edited_file_is_put_back_in_order() {
        let item = |text: &str, done| SavedItem { text: text.into(), done };
        let saved = Saved { version: VERSION, items: vec![item("a", true), item(" ", false), item("b", false)] };
        assert_eq!(texts(&Todos::from(saved)), ["b", "[x] a"]);
    }

    #[rstest]
    #[case::missing(None)]
    #[case::corrupt(Some("{"))]
    #[case::other_version(Some(r#"{"version": 99, "items": [{"text": "a"}]}"#))]
    fn an_unreadable_file_means_an_empty_list(#[case] content: Option<&str>) {
        let tmp = TempDir::new();
        let path = tmp.path().join(paths::TODOS_FILE);
        if let Some(content) = content {
            std::fs::write(&path, content).expect("write");
        }
        assert_eq!(load(&path), Todos::default());
    }
}
