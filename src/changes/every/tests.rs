use std::path::PathBuf;
use std::time::Instant;

use super::*;
use crate::changes::Mode;
use crate::changes::diff::{File, Fold, Status};
use crate::test_util::{TempDir, git_repo};

fn file(path: &str) -> Arc<File> {
    Arc::new(File {
        path: path.into(),
        old_path: None,
        status: Status::Modified,
        added: 1,
        removed: 0,
        fold: Fold::Open,
        hunks: Vec::new(),
        digest: 1,
        viewed_digest: 1,
    })
}

fn model(result: Result<Vec<&str>, &str>) -> Model {
    Model {
        mode: Mode::default(),
        base: None,
        label: None,
        result: result.map(|files| Arc::new(Diff { files: files.into_iter().map(file).collect() })).map_err(Into::into),
        digest: None,
        at: Instant::now(),
    }
}

fn project(id: u64, name: &str, workspaces: &[(u64, &TempDir)]) -> Project {
    let mut project = Project::new(id, PathBuf::from(format!("/{name}")), Some(name.into()));
    for (workspace, dir) in workspaces {
        project.workspaces.push(crate::project::Workspace::new(
            *workspace,
            dir.path().to_path_buf(),
            Some(format!("w{workspace}")),
            true,
        ));
    }
    project
}

struct Setup {
    projects: Vec<Project>,
    panel: Panel,
    _repos: Vec<TempDir>,
}

fn setup() -> Setup {
    let repos: Vec<TempDir> = (0..4).map(|i| git_repo(&[("f", &i.to_string())])).collect();
    let projects = vec![
        project(10, "api", &[(11, &repos[0]), (12, &repos[1])]),
        project(20, "blog", &[(21, &repos[2])]),
        project(30, "docs", &[(31, &repos[3])]),
    ];
    let mut panel = Panel::default();
    panel.summaries.insert(11, model(Ok(vec!["a", "b"])));
    panel.summaries.insert(12, model(Ok(vec!["c"])));
    panel.summaries.insert(21, model(Ok(vec!["d"])));
    panel.summaries.insert(31, model(Ok(vec![])));
    Setup { projects, panel, _repos: repos }
}

mod targets {
    use super::*;

    #[test]
    fn keep_the_given_order_and_only_git_checkouts_of_open_projects() {
        let mut s = setup();
        let plain = TempDir::new();
        s.projects[1].workspaces.push(crate::project::Workspace::new(22, plain.path().to_path_buf(), None, false));
        s.projects[2].closing = true;
        let found = targets(&s.projects, &[1, 0, 2], &s.panel);
        let ids: Vec<u64> = found.iter().map(|(t, _)| t.workspace).collect();
        assert_eq!(ids, [21, 11, 12]);
    }

    #[test]
    fn hide_the_workspaces_of_a_folded_project() {
        let mut s = setup();
        s.panel.headers.insert(10);
        let hidden: Vec<bool> = targets(&s.projects, &[0, 1], &s.panel).into_iter().map(|(_, h)| h).collect();
        assert_eq!(hidden, [true, true, false]);
    }

    #[test]
    fn hide_a_folded_workspace_only_when_its_header_shows() {
        let mut s = setup();
        s.panel.headers.insert(12);
        let several: Vec<bool> = targets(&s.projects, &[0], &s.panel).into_iter().map(|(_, h)| h).collect();
        assert_eq!(several, [false, true]);
        s.panel.summaries.insert(11, model(Ok(vec![])));
        let one: Vec<bool> = targets(&s.projects, &[0], &s.panel).into_iter().map(|(_, h)| h).collect();
        assert_eq!(one, [false, false]);
    }
}

mod group {
    use super::*;

    fn run(s: &Setup, order: &[usize]) -> (Body, Vec<u64>, Vec<Header>) {
        let found = targets(&s.projects, order, &s.panel);
        group(&s.projects, order, &s.panel, &found, None)
    }

    #[test]
    fn make_a_header_per_project_with_changes_in_the_given_order() {
        let s = setup();
        let (body, owners, headers) = run(&s, &[1, 0, 2]);
        let Body::Ready(diff) = body else { panic!("ready") };
        assert_eq!(diff.files.len(), 4);
        assert_eq!(owners, [21, 11, 11, 12]);
        let projects: Vec<(u64, &str)> =
            headers.iter().filter(|h| !h.workspace).map(|h| (h.id, h.label.as_str())).collect();
        assert_eq!(projects, [(20, "blog"), (10, "api")]);
        assert_eq!(headers[0].files, 0..1);
    }

    #[test]
    fn show_workspace_headers_only_when_several_have_changes() {
        let mut s = setup();
        let (_, _, headers) = run(&s, &[0, 1]);
        let workspaces: Vec<u64> = headers.iter().filter(|h| h.workspace).map(|h| h.id).collect();
        assert_eq!(workspaces, [11, 12]);
        s.panel.summaries.insert(12, model(Ok(vec![])));
        let (_, _, headers) = run(&s, &[0, 1]);
        assert!(headers.iter().all(|h| !h.workspace));
    }

    #[test]
    fn leave_out_clean_workspaces_and_projects() {
        let s = setup();
        let (_, _, headers) = run(&s, &[2, 0]);
        assert!(headers.iter().all(|h| h.id != 30));
    }

    #[test]
    fn mark_a_folded_project_header() {
        let mut s = setup();
        s.panel.headers.insert(20);
        let (_, _, headers) = run(&s, &[1, 0]);
        assert_eq!(headers.iter().map(|h| h.folded).collect::<Vec<_>>(), [true, false, false, false]);
    }

    #[test]
    fn load_while_a_target_has_no_summary_and_nothing_changed() {
        let mut s = setup();
        s.panel.summaries.clear();
        assert_eq!(run(&s, &[0, 1]).0, Body::Loading);
    }

    #[test]
    fn show_the_first_error_when_nothing_changed() {
        let mut s = setup();
        s.panel.summaries.insert(11, model(Err("boom")));
        s.panel.summaries.insert(12, model(Ok(vec![])));
        s.panel.summaries.insert(21, model(Ok(vec![])));
        assert_eq!(run(&s, &[0, 1]).0, Body::Failed("api: boom".into()));
    }
}
