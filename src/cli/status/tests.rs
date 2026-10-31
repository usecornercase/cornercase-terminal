use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::control::TabInfo;
use crate::time::Stamp;

#[test]
fn a_workspace_with_closed_agents_says_how_many() {
    let at = |closed_agents| {
        let workspace = WorkspaceInfo { name: "fix".into(), closed_agents, ..WorkspaceInfo::default() };
        workspace_parts(&workspace, None).join("  ")
    };

    assert!(at(Some(4)).ends_with("  4 agents closed"), "{}", at(Some(4)));
    assert!(at(Some(1)).ends_with("  1 agent closed"), "{}", at(Some(1)));
    assert!(!at(None).contains("closed"));
}

#[test]
fn a_workspace_says_which_workspaces_it_shares_files_with() {
    let shared = |others: &[(u64, usize)]| {
        let shared = others.iter().map(|(workspace, count)| SharedInfo {
            workspace: *workspace,
            count: *count,
            files: Vec::new(),
        });
        let workspace = WorkspaceInfo { name: "fix".into(), shared: shared.collect(), ..WorkspaceInfo::default() };
        workspace_parts(&workspace, None).join("  ")
    };

    assert!(shared(&[(12, 2)]).ends_with("  shares 2 files with workspace 12"), "{}", shared(&[(12, 2)]));
    assert!(shared(&[(12, 1)]).ends_with("  shares 1 file with workspace 12"));
    assert!(shared(&[(12, 2), (14, 3)]).ends_with("  shares 2 files with workspace 12, 3 with workspace 14"));
    assert!(!shared(&[]).contains("shares"));
}

mod the_servers_home {
    use super::*;

    #[test]
    fn is_what_paths_are_abbreviated_with() {
        let report = Report { home: Some("/home/ana".into()), ..Report::default() };

        assert_eq!(home(&report).as_deref(), Some(Path::new("/home/ana")));
        let project = ProjectInfo { path: "/home/ana/shop".into(), ..ProjectInfo::default() };
        let text = render(&Report { projects: vec![project], ..report.clone() }, home(&report).as_deref(), UNIX_EPOCH);
        assert!(text.contains("~/shop"), "{text}");
    }

    #[test]
    fn is_only_read_from_the_report_when_it_names_one() {
        let report = Report { home: None, ..Report::default() };

        assert_eq!(home(&report), std::env::var_os("HOME").map(PathBuf::from));
    }
}

mod machines {
    use super::*;

    const SEEN: &str = "2026-10-29T10:00:00.000Z";

    fn project(id: u64, name: &str, paired: Option<u64>) -> ProjectInfo {
        let tab = TabInfo { id: id * 10 + 1, name: "claude".into(), ..TabInfo::default() };
        let workspace = WorkspaceInfo {
            id: id * 10,
            name: "main".into(),
            path: format!("/home/ana/{name}").into(),
            tabs: vec![tab],
            ..WorkspaceInfo::default()
        };
        ProjectInfo {
            id,
            name: name.into(),
            path: format!("/home/ana/{name}").into(),
            machine: Some("vps".into()),
            paired: Some(paired),
            workspaces: vec![workspace],
            ..ProjectInfo::default()
        }
    }

    fn seen(name: &str, state: State, projects: Vec<ProjectInfo>) -> MachineSeen {
        MachineSeen {
            name: name.into(),
            icon: "☁".into(),
            state,
            seen: Some(SEEN.into()),
            boot: "b-1".into(),
            projects,
        }
    }

    fn rendered(machines: Vec<MachineSeen>, shown_remote: Option<ShownRemote>) -> String {
        let local = ProjectInfo { id: 3, name: "shop".into(), path: "/Users/me/shop".into(), ..ProjectInfo::default() };
        let shown = control::Ids { project: Some(3), ..control::Ids::default() };
        let report = Report { projects: vec![local], machines, shown_remote, shown, ..Report::default() };
        let now = Stamp::parse(SEEN).expect("a stamp") + Duration::from_secs(2 * 60 + 5);
        render(&report, Some(Path::new("/Users/me")), now)
    }

    #[test]
    fn come_after_what_is_here_with_their_projects_and_pairs() {
        let vps = seen("vps", State::Linked, vec![project(1, "web-shop", Some(3)), project(2, "blog", None)]);

        let text = rendered(vec![vps], None);

        assert_eq!(
            text,
            "project 3  shop  ~/shop  (shown)\n\
             machine vps  ☁  linked\n\
             \x20 project 1  web-shop  /home/ana/web-shop  paired with project 3\n\
             \x20   workspace 10  main  /home/ana/web-shop\n\
             \x20     tab 11  claude\n\
             \x20 project 2  blog  /home/ana/blog\n\
             \x20   workspace 20  main  /home/ana/blog\n\
             \x20     tab 21  claude\n"
        );
    }

    #[test]
    fn one_that_is_down_says_when_it_was_seen() {
        let pi = seen("pi", State::Unreachable, Vec::new());

        assert!(rendered(vec![pi], None).ends_with("machine pi  ☁  unreachable · seen 2 min ago\n"));
    }

    #[test]
    fn one_never_seen_up_is_only_unreachable() {
        let pi = MachineSeen { seen: None, ..seen("pi", State::Unreachable, Vec::new()) };

        assert!(rendered(vec![pi], None).ends_with("machine pi  ☁  unreachable\n"));
    }

    #[test]
    fn what_is_shown_there_is_marked_on_its_machine_only_and_not_here() {
        let machines = vec![
            seen("vps", State::Linked, vec![project(1, "web-shop", Some(3))]),
            seen("pi", State::Linked, vec![project(1, "web-shop", None)]),
        ];
        let shown = ShownRemote {
            machine: "vps".into(),
            boot: "b-1".into(),
            project: 1,
            workspace: 10,
            tab: Some(11),
            path: "/home/ana/web-shop".into(),
        };

        let text = rendered(machines, Some(shown));

        assert!(text.starts_with("project 3  shop  ~/shop\n"), "{text}");
        assert_eq!(text.matches("(shown)").count(), 3, "{text}");
        assert!(text.contains("  tab 11  claude  (shown)\nmachine pi"), "{text}");
    }
}
