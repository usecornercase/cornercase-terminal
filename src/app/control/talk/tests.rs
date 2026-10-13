use super::CANNOT_READ;
use crate::agents::Known;

#[test]
fn the_last_message_refusal_names_every_known_agent() {
    for agent in Known::ALL {
        assert!(CANNOT_READ.contains(agent.title()), "missing {}", agent.title());
    }
}
