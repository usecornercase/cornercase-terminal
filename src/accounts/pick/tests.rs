use super::AgentAccounts;

fn claude(active: &str) -> AgentAccounts {
    let labels = vec!["System".into(), "home".into(), "work".into()];
    AgentAccounts::new("claude".into(), &["home".into(), "work".into()], labels, active)
}

mod choosing {
    use super::claude;

    #[test]
    fn an_account_that_exists_is_chosen() {
        let agent = claude("work");
        assert_eq!(agent.chosen(), "work");
        assert_eq!(agent.missing(), None);
    }

    #[test]
    fn the_agents_own_login_is_the_empty_name() {
        assert_eq!(claude("").chosen(), "");
    }

    #[test]
    fn an_account_in_use_that_is_gone_is_the_agents_own_login() {
        let agent = claude("old");
        assert_eq!((agent.chosen(), agent.missing()), ("", None));
    }

    #[test]
    fn a_saved_account_that_exists_is_chosen() {
        let agent = claude("work").keeping(Some("home"));
        assert_eq!((agent.chosen(), agent.missing()), ("home", None));
    }

    #[test]
    fn a_saved_account_that_is_gone_stays_chosen_until_another_is_picked() {
        let mut agent = claude("work").keeping(Some("old"));
        assert_eq!(agent.combo.chosen(), None);
        assert_eq!(agent.chosen(), "old");
        assert_eq!(agent.missing(), Some("old"));
        agent.combo.choose(0);
        assert_eq!(agent.chosen(), "");
        assert_eq!(agent.missing(), None);
    }

    #[test]
    fn a_name_picks_its_account() {
        let mut agent = claude("");
        agent.choose_named("home");
        assert_eq!(agent.chosen(), "home");
        agent.choose_named("nobody");
        assert_eq!(agent.chosen(), "home");
    }
}
