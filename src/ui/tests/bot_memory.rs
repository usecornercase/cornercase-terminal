use super::*;
use crate::editor::Editor;
use crate::ui::bot_memory::{BotMemory, Hit, hit};

fn memory(text: &str, text_focused: bool, note: Option<Note>) -> BotMemory {
    BotMemory {
        title: "Memory › PRs".into(),
        intro:
            "What PRs keeps from one conversation to the next, in memory.md. A new conversation reads its first 8 KB, \
                and the bot keeps it up to date itself."
                .into(),
        text: Editor::new(text),
        text_focused,
        note,
        hint: "enter breaks the line · tab moves to save · esc cancels".into(),
    }
}

fn shown(memory: BotMemory) -> View<'static> {
    View { overlay: Some(Overlay::BotMemory(Box::new(memory))), ..view(&["~"]) }
}

#[test]
fn the_memory_is_drawn_under_what_it_is_for() {
    let kept = "- prefers squash merges\n- runs `cargo test` before a PR\n";
    insta::assert_snapshot!(render(&shown(memory(kept, true, None))).backend());
}

#[test]
fn an_error_replaces_the_hint() {
    let note = Some(Note::Error("PRs changed its memory meanwhile: save again to replace it with yours".into()));
    insta::assert_snapshot!(render(&shown(memory("- mine", false, note))).backend());
}

#[test]
fn the_field_and_both_buttons_are_targets() {
    let spots = (0..AREA.height).flat_map(|y| (0..AREA.width).map(move |x| Position::new(x, y)));
    let hits: Vec<Hit> = spots.filter_map(|pos| hit(AREA, pos)).collect();
    assert!(hits.contains(&Hit::Cancel) && hits.contains(&Hit::Save));
    assert!(hits.contains(&Hit::Text(0, 0)));
}
