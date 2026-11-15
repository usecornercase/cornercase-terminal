use super::{CANNOT_READ, exited_text};
use crate::agents::Known;

#[test]
fn the_last_message_refusal_names_every_known_agent() {
    for agent in Known::ALL {
        assert!(CANNOT_READ.contains(agent.title()), "missing {}", agent.title());
    }
}

#[test]
fn the_exited_role_refusal_names_the_pane_the_role_and_the_way_back() {
    let text = exited_text(7, "coder", 3);

    assert!(text.contains("pane 7 is coder in a flow's run"), "{text}");
    assert!(text.contains("its agent exited"), "{text}");
    assert!(text.contains("`cornercase agents reopen --workspace 3`"), "{text}");
}
