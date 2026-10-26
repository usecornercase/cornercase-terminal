use rstest::rstest;
use serde_json::{Value, json};

use super::*;

fn todo(app: &mut App, args: &Value) -> Value {
    ok(app, "todo", args)
}

fn add(app: &mut App, text: &str) -> u64 {
    todo(app, &json!({"add": text}))["todo"].as_u64().expect("an id")
}

fn texts(app: &App) -> Vec<String> {
    app.panels.todos.items().iter().map(|i| if i.done { format!("[x] {}", i.text) } else { i.text.clone() }).collect()
}

fn undo_of(answer: &Value) -> u64 {
    answer["undo"].as_u64().expect("an undo number")
}

mod editing {
    use super::*;

    #[test]
    fn edit_replaces_the_text() {
        let (mut app, _rx) = app();
        let id = add(&mut app, "fix logn");
        todo(&mut app, &json!({"edit": {"id": id, "text": "fix login"}}));
        assert_eq!(texts(&app), ["fix login"]);
    }

    #[test]
    fn a_blank_text_is_refused_like_add_does() {
        let (mut app, _rx) = app();
        let id = add(&mut app, "fix login");
        let refusal = refused(&mut app, "todo", &json!({"edit": {"id": id, "text": " "}}));
        assert_eq!((refusal.message.as_str(), texts(&app)), ("the todo needs a text", vec!["fix login".to_string()]));
    }

    #[test]
    fn an_edit_closes_the_panel_field_on_that_item() {
        let (mut app, _rx) = app();
        let id = add(&mut app, "a");
        let editor = Editor::new("a");
        app.panels.todo.start_editing(id, editor);
        todo(&mut app, &json!({"edit": {"id": id, "text": "b"}}));
        assert!(app.panels.todo.field().is_none());
        assert_eq!(texts(&app), ["b"]);
    }

    #[test]
    fn an_unknown_item_is_not_found() {
        let (mut app, _rx) = app();
        assert_eq!(refused(&mut app, "todo", &json!({"edit": {"id": 9, "text": "x"}})).code, "not-found");
    }
}

mod checking {
    use super::*;

    #[test]
    fn undone_puts_back_a_done_item_and_leaves_a_pending_one() {
        let (mut app, _rx) = app();
        let a = add(&mut app, "a");
        let b = add(&mut app, "b");
        let c = add(&mut app, "c");
        todo(&mut app, &json!({"done": a}));
        todo(&mut app, &json!({"done": b}));
        todo(&mut app, &json!({"undone": a}));
        todo(&mut app, &json!({"undone": c}));
        assert_eq!(texts(&app), ["c", "a", "[x] b"]);
    }
}

mod moving {
    use super::*;

    #[rstest]
    #[case::among_pending(0, Some(2), &["b", "a", "c", "[x] d"])]
    #[case::to_the_end(0, None, &["b", "c", "a", "[x] d"])]
    #[case::before_a_done_item(0, Some(3), &["b", "c", "a", "[x] d"])]
    fn items_stay_among_their_kind(#[case] item: usize, #[case] before: Option<usize>, #[case] expected: &[&str]) {
        let (mut app, _rx) = app();
        let ids: Vec<u64> = ["a", "b", "c", "d"].iter().map(|t| add(&mut app, t)).collect();
        todo(&mut app, &json!({"done": ids[3]}));
        let place = before.map(|i| ids[i]);
        todo(&mut app, &json!({"move": {"id": ids[item], "before": place}}));
        assert_eq!(texts(&app), expected);
    }

    #[test]
    fn before_itself_is_malformed() {
        let (mut app, _rx) = app();
        let id = add(&mut app, "a");
        assert_eq!(refused(&mut app, "todo", &json!({"move": {"id": id, "before": id}})).code, "malformed");
    }

    #[test]
    fn an_unknown_place_is_not_found() {
        let (mut app, _rx) = app();
        let id = add(&mut app, "a");
        assert_eq!(refused(&mut app, "todo", &json!({"move": {"id": id, "before": 99}})).code, "not-found");
    }
}

mod removing {
    use super::*;

    #[test]
    fn rm_answers_its_undo_and_restore_puts_it_back_where_it_was() {
        let (mut app, _rx) = app();
        let ids: Vec<u64> = ["a", "b", "c"].iter().map(|t| add(&mut app, t)).collect();
        let removed = todo(&mut app, &json!({"rm": ids[1]}));
        assert_eq!((removed["todos"][0]["text"].as_str(), texts(&app)), (Some("b"), vec!["a".into(), "c".into()]));

        let restored = todo(&mut app, &json!({"restore": undo_of(&removed)}));

        assert_eq!(
            (restored["todos"][0]["id"].as_u64(), texts(&app)),
            (Some(ids[1]), ["a", "b", "c"].map(String::from).to_vec())
        );
        assert_eq!(refused(&mut app, "todo", &json!({"restore": undo_of(&removed)})).code, "not-found");
    }

    #[test]
    fn clear_done_with_nothing_done_removes_nothing() {
        let (mut app, _rx) = app();
        add(&mut app, "a");
        let answer = todo(&mut app, &json!("clear-done"));
        assert_eq!((answer["todos"].as_array().map(Vec::len), answer.get("undo")), (Some(0), None));
    }

    #[test]
    fn clear_done_restores_every_item_it_removed() {
        let (mut app, _rx) = app();
        let a = add(&mut app, "a");
        add(&mut app, "b");
        let c = add(&mut app, "c");
        todo(&mut app, &json!({"done": a}));
        todo(&mut app, &json!({"done": c}));
        let cleared = todo(&mut app, &json!("clear-done"));
        todo(&mut app, &json!({"restore": undo_of(&cleared)}));
        assert_eq!(texts(&app), ["b", "[x] a", "[x] c"]);
    }

    #[test]
    fn only_the_last_16_removals_can_be_undone() {
        let (mut app, _rx) = app();
        let mut undos = Vec::new();
        for i in 0..17 {
            let id = add(&mut app, &format!("item {i}"));
            undos.push(undo_of(&todo(&mut app, &json!({"rm": id}))));
        }
        assert_eq!(refused(&mut app, "todo", &json!({"restore": undos[0]})).code, "not-found");
        todo(&mut app, &json!({"restore": undos[1]}));
        assert_eq!(texts(&app), ["item 1"]);
    }

    #[test]
    fn a_removal_closes_the_panel_field_on_that_item() {
        let (mut app, _rx) = app();
        let id = add(&mut app, "a");
        let editor = Editor::new("a");
        app.panels.todo.start_editing(id, editor);
        todo(&mut app, &json!({"rm": id}));
        assert!(app.panels.todo.field().is_none());
    }
}
