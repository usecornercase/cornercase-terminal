use serde_json::json;

use super::*;

fn page(messages: &Value, more_before: bool) -> Value {
    json!({ "pane": 7, "agent": "claude", "conversation": "3f2a", "messages": messages, "more_before": more_before })
}

#[test]
fn each_message_is_a_block_under_its_id_role_and_time() {
    let value = page(
        &json!([
            { "id": 41, "role": "user", "text": "Add the login page", "written": "2026-11-10T09:12:03Z" },
            { "id": 42, "role": "assistant", "text": "Done.\nThe page is in src/login.rs.", "written": null },
        ]),
        true,
    );

    assert_eq!(
        text(&value),
        "41  user  2026-11-10T09:12:03Z\nAdd the login page\n\n42  assistant\nDone.\nThe page is in src/login.rs.\n"
    );
}

#[test]
fn an_empty_page_says_so() {
    assert_eq!(text(&page(&json!([]), true)), "no messages\n");
}

#[test]
fn earlier_messages_are_named_by_the_first_id_shown() {
    let messages = json!([{ "id": 41, "role": "user", "text": "Hi", "written": null }]);

    assert_eq!(earlier(&page(&messages, true)).as_deref(), Some("earlier messages: --before 41\n"));
    assert_eq!(earlier(&page(&messages, false)), None);
    assert_eq!(earlier(&page(&json!([]), true)), None);
}
