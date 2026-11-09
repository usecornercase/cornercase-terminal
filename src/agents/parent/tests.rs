use rstest::rstest;

use super::*;

fn state(kind: &str, id: &str) -> AgentState {
    AgentState { kind: kind.into(), conversation: id.into(), mode: None, account: None }
}

fn conversation(id: &str) -> Conversation {
    Conversation { kind: "claude".into(), conversation: id.into() }
}

mod made {
    use super::*;

    #[rstest]
    #[case::the_conversation_read_from_the_agent_in_front(
        Some(3),
        Some(3),
        Some(1),
        By::Conversation(conversation("s1"))
    )]
    #[case::an_agent_whose_conversation_is_unread(None, Some(3), Some(1), By::Agent(3))]
    #[case::a_conversation_read_from_an_earlier_process(Some(2), Some(3), Some(1), By::Agent(3))]
    #[case::no_agent(None, None, Some(1), By::Shell(1))]
    fn it_names_what_ran_when_it_asked(
        #[case] read: Option<i32>,
        #[case] agent: Option<i32>,
        #[case] shell: Option<i32>,
        #[case] by: By,
    ) {
        let resume = state("claude", "s1");
        let running = Running { conversation: read.map(|_| &resume), read, agent, shell };

        assert_eq!(running.by(), Some(by));
    }

    #[test]
    fn nothing_runs_without_a_shell() {
        assert_eq!(Running::default().by(), None);
    }
}

mod holds {
    use super::*;

    fn now(resume: Option<&AgentState>, agent: Option<i32>, shell: Option<i32>) -> Running<'_> {
        Running { conversation: resume, read: None, agent, shell }
    }

    #[rstest]
    #[case::the_same_conversation(Some(("claude", "s1")), Some(4), true)]
    #[case::the_same_conversation_resuming(Some(("claude", "s1")), None, true)]
    #[case::another_conversation(Some(("claude", "s2")), Some(4), false)]
    #[case::none(None, Some(4), false)]
    #[case::the_same_id_under_another_kind(Some(("codex", "s1")), Some(4), false)]
    fn a_conversation(#[case] resume: Option<(&str, &str)>, #[case] agent: Option<i32>, #[case] holds: bool) {
        let resume = resume.map(|(kind, id)| state(kind, id));

        assert_eq!(By::Conversation(conversation("s1")).holds(now(resume.as_ref(), agent, Some(1))), holds);
    }

    #[rstest]
    #[case::the_same_process(Some(4), None, true)]
    #[case::the_same_process_once_a_conversation_is_known(Some(4), Some(("claude", "s1")), true)]
    #[case::another_process(Some(5), None, false)]
    #[case::an_agent_that_exited(None, None, false)]
    fn an_agent(#[case] agent: Option<i32>, #[case] resume: Option<(&str, &str)>, #[case] holds: bool) {
        let resume = resume.map(|(kind, id)| state(kind, id));

        assert_eq!(By::Agent(4).holds(now(resume.as_ref(), agent, Some(1))), holds);
    }

    #[rstest]
    #[case::its_shell_with_nothing_running(Some(1), None, false, true)]
    #[case::an_agent_in_front(Some(1), Some(4), false, false)]
    #[case::a_conversation_resuming(Some(1), None, true, false)]
    #[case::a_respawned_shell(Some(2), None, false, false)]
    fn a_shell(#[case] shell: Option<i32>, #[case] agent: Option<i32>, #[case] resuming: bool, #[case] holds: bool) {
        let resume = state("claude", "s1");

        assert_eq!(By::Shell(1).holds(now(resuming.then_some(&resume), agent, shell)), holds);
    }

    #[test]
    fn a_pane_always() {
        assert!(By::Pane.holds(Running::default()));
    }
}

mod learn {
    use super::*;

    #[test]
    fn an_agent_takes_the_conversation_read_from_its_process() {
        let mut parent = Parent { pane: 1, by: By::Agent(4) };

        parent.learn((4, &conversation("s1")));

        assert_eq!(parent.by, By::Conversation(conversation("s1")));
    }

    #[rstest]
    #[case::another_process(By::Agent(4), 5)]
    #[case::a_shell(By::Shell(4), 4)]
    #[case::a_conversation(By::Conversation(conversation("s0")), 4)]
    #[case::a_pane(By::Pane, 4)]
    fn nothing_else_changes(#[case] by: By, #[case] pid: i32) {
        let mut parent = Parent { pane: 1, by: by.clone() };

        parent.learn((pid, &conversation("s1")));

        assert_eq!(parent.by, by);
    }
}
