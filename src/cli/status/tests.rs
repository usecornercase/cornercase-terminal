use super::*;

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

mod the_servers_home {
    use super::*;

    #[test]
    fn is_what_paths_are_abbreviated_with() {
        let report = Report { home: Some("/home/ana".into()), ..Report::default() };

        assert_eq!(home(&report).as_deref(), Some(Path::new("/home/ana")));
        let project = ProjectInfo { path: "/home/ana/shop".into(), ..ProjectInfo::default() };
        let text = render(&Report { projects: vec![project], ..report.clone() }, home(&report).as_deref());
        assert!(text.contains("~/shop"), "{text}");
    }

    #[test]
    fn is_only_read_from_the_report_when_it_names_one() {
        let report = Report { home: None, ..Report::default() };

        assert_eq!(home(&report), std::env::var_os("HOME").map(PathBuf::from));
    }
}
