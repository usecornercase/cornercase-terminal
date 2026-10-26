#[test]
fn the_skill_names_every_known_agent() {
    for agent in crate::agents::Known::ALL {
        assert!(super::SKILL.contains(agent.title()), "missing {}", agent.title());
    }
}
