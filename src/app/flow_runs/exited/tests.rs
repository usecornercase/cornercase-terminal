use super::*;

fn roles(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

mod words {
    use super::*;

    #[test]
    fn one_role_is_named_with_its_agent() {
        assert_eq!(exit_words(&roles(&["coder"]), "shop › fix"), "coder's agent exited in shop › fix");
    }

    #[test]
    fn several_roles_come_after_the_count_and_the_place() {
        assert_eq!(
            exit_words(&roles(&["orchestrator", "coder", "reviewer"]), "shop › fix"),
            "3 agents exited in shop › fix: orchestrator, coder and reviewer"
        );
        assert_eq!(exit_words(&roles(&["coder", "reviewer"]), "shop"), "2 agents exited in shop: coder and reviewer");
    }
}
