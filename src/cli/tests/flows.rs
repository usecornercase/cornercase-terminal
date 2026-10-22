use super::args::ImportTo;
use super::flow::{confirmation, fetched, folder_note, imported_line, listing, started};
use crate::control::{BrokenFlow, FlowInfo, Flows, Imported, MemberInfo, RoleInfo, RunInfo};
use crate::flows::Worktree;
use crate::flows::registry::Source;

#[test]
fn a_fetched_flow_says_where_it_is_from_and_that_nobody_reviewed_it() {
    let said = fetched(&Source::Repo("octocat/audit".into()), None);
    assert_eq!(
        said,
        "from octocat/audit, which nobody here reviewed: its prompts drive agents that never ask for permission; read them above or at https://github.com/octocat/audit"
    );
    let said = fetched(&Source::Url("https://x.dev/a.toml".into()), Some("it sets A for `coder`".into()));
    assert!(said.starts_with("from https://x.dev/a.toml, which") && said.ends_with("; it sets A for `coder`"));
}

#[test]
fn an_import_is_confirmed_with_what_it_sets_and_reported_with_where_it_went() {
    let question = confirmation("it sets A for `coder`; they run without asking, so check them", ImportTo::Yours);
    assert_eq!(
        question,
        "it sets A for `coder`; they run without asking, so check them\nImport it into your flows? [y/N] "
    );
    assert!(confirmation("x", ImportTo::Project).ends_with("into the project's flows? [y/N] "));
    let done = Imported {
        name: "Pair".into(),
        origin: "user".into(),
        path: Some("/c/flows/pair.toml".into()),
        replaced: true,
        note: None,
    };
    assert_eq!(imported_line(&done), "imported Pair into yours (replaced): /c/flows/pair.toml\n");
}

fn role(role: &str, agent: &str, model: Option<&str>, edits: bool) -> RoleInfo {
    RoleInfo {
        role: role.into(),
        agent: agent.into(),
        model: model.map(str::to_string),
        effort: None,
        edits,
        worktree: Worktree::Shared,
    }
}

#[test]
fn the_list_names_each_flow_its_origin_its_roles_and_its_problems() {
    let flows = Flows {
        flows: vec![
            FlowInfo {
                name: "Quick".into(),
                origin: "built-in".into(),
                description: "A coder and a reviewer.".into(),
                roles: vec![role("orchestrator", "claude", None, false), role("coder", "codex", Some("gpt-5.5"), true)],
                ..FlowInfo::default()
            },
            FlowInfo {
                name: "Mine".into(),
                origin: "project".into(),
                path: Some("/r/.cornercase/flows/mine.toml".into()),
                roles: vec![role("orchestrator", "vim", None, false)],
                problems: vec!["the flow has no stages".into()],
                ..FlowInfo::default()
            },
        ],
        broken: vec![BrokenFlow { path: "/c/flows/x.toml".into(), error: "expected `=`".into() }],
        ..Flows::default()
    };

    assert_eq!(
        listing(&flows),
        "Quick (built in)\n  A coder and a reviewer.\n  orchestrator: claude, coder: codex · gpt-5.5 (edits)\n\
         Mine (this project, /r/.cornercase/flows/mine.toml)\n  orchestrator: vim\n  cannot run: the flow has no \
         stages\ncannot read /c/flows/x.toml: expected `=`\n"
    );
}

#[test]
fn a_run_prints_its_workspace_then_a_pane_and_a_role_per_line() {
    let member = |role: &str, pane| MemberInfo {
        role: role.into(),
        agent: "claude".into(),
        tab: 0,
        pane,
        worktree: None,
        branch: None,
    };
    let run = RunInfo {
        workspace: 9,
        members: vec![member("orchestrator", 12), member("second reviewer", 14)],
        ..RunInfo::default()
    };

    assert_eq!(started(&run), "9\n12\torchestrator\n14\tsecond reviewer\n");
}

#[test]
fn a_run_names_its_folder_on_stderr_when_it_has_one() {
    let run = RunInfo { run_dir: Some("/s/runs/2026-10-17-api".into()), ..RunInfo::default() };
    assert_eq!(folder_note(&run).as_deref(), Some("the run keeps its files in /s/runs/2026-10-17-api"));
    assert_eq!(folder_note(&RunInfo::default()), None);
}
