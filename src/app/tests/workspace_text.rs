use rstest::rstest;

use crate::app::dialogs::close_question;
use crate::app::worktrees::{Removing, removal_question};

fn removing(roles: usize, teardown: bool, on: Option<&'static str>) -> Removing<'static> {
    Removing { label: "api", path: "~/wt/api", roles, teardown, closed: "", on }
}

mod the_removal_question {
    use super::*;

    #[rstest]
    #[case::here(
        removing(0, false, None),
        "Remove the workspace api and delete its worktree folder ~/wt/api? The branch is kept."
    )]
    #[case::on_a_machine(
        removing(0, false, Some("vps")),
        "Remove the workspace api on vps and delete its worktree folder ~/wt/api? The branch is kept."
    )]
    #[case::on_a_machine_with_roles_and_a_teardown(
        removing(2, true, Some("vps")),
        "Remove the workspace api on vps and delete its worktree folder ~/wt/api, with the worktrees of its 2 roles? \
         Its teardown runs first. The branches are kept."
    )]
    fn names_the_machine_only_when_there_is_one(#[case] removing: Removing<'static>, #[case] expected: &str) {
        assert_eq!(removal_question(&removing), expected);
    }

    #[test]
    fn ends_with_the_closed_agents_it_forgets() {
        let removing = Removing { closed: " Its closed agent is forgotten.", ..removing(0, false, Some("vps")) };

        assert!(removal_question(&removing).ends_with("The branch is kept. Its closed agent is forgotten."));
    }
}

mod the_close_question {
    use super::*;

    #[rstest]
    #[case::here(None, 0, "", "Close the workspace api?")]
    #[case::on_a_machine(
        Some("vps"),
        1,
        "",
        "Close the workspace api on vps? Its tab and the programs running in it are stopped."
    )]
    #[case::on_a_machine_with_closed_agents(
        Some("vps"),
        3,
        " Its 2 closed agents are forgotten.",
        "Close the workspace api on vps? Its 3 tabs and the programs running in them are stopped. \
         Its 2 closed agents are forgotten."
    )]
    fn names_the_machine_only_when_there_is_one(
        #[case] on: Option<&str>,
        #[case] tabs: usize,
        #[case] closed: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(close_question("api", tabs, closed, on), expected);
    }
}
