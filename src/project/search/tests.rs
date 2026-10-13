use rstest::rstest;

use super::*;
use crate::project::group::Group;

fn candidate(kind: Kind, name: &str, keys: &[&str]) -> Candidate {
    Candidate {
        kind,
        goto: Goto::Group(1),
        name: name.into(),
        context: String::new(),
        keys: keys.iter().map(|k| (*k).to_string()).collect(),
    }
}

fn names(results: &[Candidate]) -> Vec<&str> {
    results.iter().map(|c| c.name.as_str()).collect()
}

mod rank {
    use super::*;

    #[test]
    fn an_empty_query_finds_nothing() {
        assert_eq!(rank(vec![candidate(Kind::Project, "api", &["api"])], "  "), Vec::<Candidate>::new());
    }

    #[test]
    fn leaves_out_what_does_not_match() {
        let found =
            rank(vec![candidate(Kind::Project, "api", &["api"]), candidate(Kind::Project, "web", &["web"])], "we");
        assert_eq!(names(&found), ["web"]);
    }

    #[test]
    fn exact_then_prefix_then_substring() {
        let found = rank(
            vec![
                candidate(Kind::Project, "my-api", &["my-api"]),
                candidate(Kind::Project, "api-docs", &["api-docs"]),
                candidate(Kind::Project, "api", &["api"]),
            ],
            "api",
        );
        assert_eq!(names(&found), ["api", "api-docs", "my-api"]);
    }

    #[test]
    fn projects_then_workspaces_then_tabs_on_a_tie() {
        let found = rank(
            vec![
                candidate(Kind::Tab, "login-tab", &["login-tab"]),
                candidate(Kind::Workspace, "login-ws", &["login-ws"]),
                candidate(Kind::Project, "login-app", &["login-app"]),
            ],
            "login",
        );
        assert_eq!(names(&found), ["login-app", "login-ws", "login-tab"]);
    }

    #[test]
    fn keeps_the_given_order_otherwise() {
        let found = rank(
            vec![candidate(Kind::Tab, "zsh", &["zsh", "feat/b"]), candidate(Kind::Tab, "nvim", &["nvim", "feat/a"])],
            "feat",
        );
        assert_eq!(names(&found), ["zsh", "nvim"]);
    }

    #[test]
    fn any_key_matches_and_the_best_one_counts() {
        let found = rank(
            vec![
                candidate(Kind::Workspace, "renamed", &["renamed", "feat/x"]),
                candidate(Kind::Workspace, "other", &["other", "x"]),
            ],
            "x",
        );
        assert_eq!(names(&found), ["other", "renamed"]);
    }

    #[test]
    fn ignores_case() {
        assert_eq!(names(&rank(vec![candidate(Kind::Project, "API", &["API"])], "api")), ["API"]);
    }
}

mod find {
    use super::*;

    #[rstest]
    #[case::start("feat/search", "feat", Some((0, 4)))]
    #[case::middle("my-api", "API", Some((3, 6)))]
    #[case::counts_chars_not_bytes("ñandú-api", "api", Some((6, 9)))]
    #[case::missing("zsh", "vim", None)]
    #[case::empty("zsh", "", None)]
    fn returns_the_char_range_of_the_first_match(
        #[case] haystack: &str,
        #[case] needle: &str,
        #[case] expected: Option<(usize, usize)>,
    ) {
        assert_eq!(find(haystack, needle), expected);
    }
}

mod candidates {
    use std::path::PathBuf;

    use super::*;
    use crate::config::Config;
    use crate::project::group::GroupEntry;
    use crate::project::split::Node;
    use crate::project::{Phase, Project, Tab, Workspace};
    use crate::test_util::{git, git_repo};

    fn group(id: u64, name: &str) -> Group {
        Group { id, entry: GroupEntry { name: name.into(), icon: '●', colour: 4, collapsed: false } }
    }

    fn named_tab(id: u64, name: &str) -> Tab {
        Tab {
            id,
            name: Some(name.into()),
            panes: Vec::new(),
            active: 0,
            layout: Node::Leaf(0),
            right_clicks: Vec::new(),
        }
    }

    fn project(id: u64, name: &str, group: Option<u64>) -> Project {
        let mut project = Project::new(id, PathBuf::from(format!("/{name}")), Some(name.into()));
        project.group = group;
        project
    }

    fn all(groups: &[Group], projects: &[Project]) -> Vec<Candidate> {
        candidates(groups, projects, &Config::default(), None)
    }

    #[test]
    fn put_groups_first_and_give_a_project_its_groups_name_as_context() {
        let groups = [group(1, "work")];
        let found = all(&groups, &[project(10, "api", Some(1)), project(11, "blog", None)]);
        assert_eq!(names(&found), ["● work", "api", "blog"]);
        assert_eq!(found[0].kind, Kind::Group);
        assert_eq!((found[1].context.as_str(), found[2].context.as_str()), ("work", ""));
    }

    #[test]
    fn leave_out_workspaces_that_are_closing_or_being_removed() {
        let mut p = project(10, "api", None);
        for (id, phase) in [(11, Phase::Open), (12, Phase::Closing), (13, Phase::Removing("gone".into()))] {
            let mut w = Workspace::new(id, PathBuf::from(format!("/api/{id}")), Some(format!("w{id}")), false);
            w.phase = phase;
            p.workspaces.push(w);
        }
        let found = all(&[], &[p]);
        assert_eq!(names(&found), ["api", "w11"]);
    }

    #[test]
    fn find_a_named_workspace_by_its_branch_too() {
        let repo = git_repo(&[("a", "a")]);
        git(repo.path(), &["checkout", "--quiet", "-b", "fix/login"]);
        let mut p = project(10, "api", None);
        p.workspaces.push(Workspace::new(11, repo.path().to_path_buf(), Some("login".into()), true));
        let found = all(&[], &[p]);
        assert_eq!(found[1].keys, ["login", "fix/login"]);
    }

    #[test]
    fn give_a_tab_the_project_and_workspace_as_context() {
        let mut p = project(10, "api", None);
        let mut w = Workspace::new(11, PathBuf::from("/api/w"), Some("login".into()), false);
        w.tabs.push(named_tab(12, "server"));
        p.workspaces.push(w);
        let found = all(&[], &[p]);
        let tab = found.last().expect("a tab");
        assert_eq!((tab.kind, tab.context.as_str()), (Kind::Tab, "api › login"));
        assert_eq!(tab.goto, Goto::Place { project: 10, workspace: Some(11), tab: Some(12) });
    }
}
