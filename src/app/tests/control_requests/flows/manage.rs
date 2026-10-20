use serde_json::{Value, json};

use super::*;
use crate::control::{FlowRemoved, FlowSaved, FlowSpec, Flows};
use crate::flows::share;
use crate::flows::store::Origin;

fn spec(flow: &Flow) -> Value {
    serde_json::to_value(FlowSpec::from(flow)).expect("a spec")
}

fn yours(team: &Team) -> PathBuf {
    paths::flows(&team.config.path().join("config.json"))
}

fn project(team: &Team) -> PathBuf {
    store::project_dir(team.repo.path())
}

fn listed(team: &mut Team) -> Flows {
    serde_json::from_value(ok(&mut team.app, "flow-list", &json!({}))).expect("flows")
}

fn saved(team: &mut Team, flow: &Flow, extra: &Value) -> FlowSaved {
    let mut args = json!({"flow": spec(flow), "place": "yours", "path": null, "check": false});
    args.as_object_mut().expect("args").extend(extra.as_object().expect("extra").clone());
    serde_json::from_value(ok(&mut team.app, "flow-save", &args)).expect("a save")
}

fn save_refused(team: &mut Team, flow: &Flow, extra: &Value) -> Refused {
    let mut args = json!({"flow": spec(flow), "place": "yours", "path": null, "check": false});
    args.as_object_mut().expect("args").extend(extra.as_object().expect("extra").clone());
    refused(&mut team.app, "flow-save", &args)
}

fn removed(value: Value) -> FlowRemoved {
    serde_json::from_value(value).expect("a removal")
}

fn named(name: &str) -> Flow {
    Flow { name: name.into(), ..pair() }
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn flows_are_listed_whole_with_the_deleted_built_ins_and_the_reserved_names() {
    let mut team = Team::new();
    ok(&mut team.app, "flow-hide", &json!({"name": "Quick"}));

    let flows = listed(&mut team);

    let pair_flow = flows.flows.iter().find(|f| f.name == "Pair").expect("Pair");
    assert_eq!(pair_flow.flow, FlowSpec::from(&pair()));
    assert!(!pair_flow.edited);
    assert!(flows.flows.iter().all(|f| f.name != "Quick"));
    let hidden: Vec<&str> = flows.hidden.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(hidden, ["Quick"]);
    assert_eq!(flows.hidden[0].origin, "built-in");
    assert_eq!(flows.hidden[0].path, None);
    assert!(flows.builtins.iter().any(|name| name == "Quick"));
    assert!(flows.builtins.iter().any(|name| name == "Arena"));
}

#[test]
fn your_version_of_a_built_in_is_marked_edited() {
    let mut team = Team::new();
    saved(&mut team, &named("Quick"), &json!({}));

    let flows = listed(&mut team);

    let mine = flows.flows.iter().find(|f| f.name == "Quick" && f.origin == "user").expect("yours");
    assert!(mine.edited);
}

#[test]
fn a_draft_is_checked_without_being_written() {
    let mut team = Team::new();
    let before = files(&yours(&team));

    let checked = saved(&mut team, &named("Draft"), &json!({"check": true}));

    assert!(!checked.saved);
    assert_eq!(checked.path, None);
    assert!(checked.problems.is_empty(), "{:?}", checked.problems);
    assert_eq!(files(&yours(&team)), before);
}

#[test]
fn a_checked_draft_names_its_problems_and_a_conflict() {
    let mut team = Team::new();
    let mut draft = named("Draft");
    draft.stages.clear();

    let checked = saved(&mut team, &draft, &json!({"check": true}));

    assert!(checked.problems.iter().any(|p| p.contains("no stages")), "{:?}", checked.problems);
    let clash = saved(&mut team, &named("Pair"), &json!({"place": "project", "check": true}));
    assert!(clash.problems.iter().any(|p| p.contains("runs instead")), "{:?}", clash.problems);
}

#[test]
fn a_draft_with_problems_is_refused_as_malformed() {
    let mut team = Team::new();
    let mut draft = named("Draft");
    draft.stages.clear();
    let before = files(&yours(&team));

    let refused = refused(
        &mut team.app,
        "flow-save",
        &json!({"flow": spec(&draft), "place": "yours", "path": null, "check": false}),
    );

    assert_eq!(refused.code, "malformed");
    assert!(refused.message.contains("the flow has problems"), "{}", refused.message);
    assert_eq!(files(&yours(&team)), before);
}

#[test]
fn a_draft_is_saved_and_renamed_by_its_previous_path() {
    let mut team = Team::new();
    let first = saved(&mut team, &named("Draft"), &json!({}));
    let old = first.path.clone().expect("a path");
    assert!(first.saved && old.is_file());

    let renamed = saved(&mut team, &named("Final"), &json!({"path": old}));

    let new = renamed.path.expect("a path");
    assert!(new.is_file() && !old.exists(), "{new:?} {old:?}");
    assert_eq!(store::read(&new).expect("the flow").name, "Final");
}

#[test]
fn a_draft_moves_from_the_project_to_yours() {
    let mut team = Team::new();
    let theirs = saved(&mut team, &named("Shared"), &json!({"place": "project"}));
    let old = theirs.path.expect("a path");
    assert!(old.starts_with(project(&team)), "{old:?}");
    assert_eq!(theirs.origin, "project");

    let moved = saved(&mut team, &named("Shared"), &json!({"path": old}));

    assert_eq!(moved.origin, "user");
    assert!(moved.problems.is_empty(), "{:?}", moved.problems);
    assert!(!old.exists());
    assert!(moved.path.expect("a path").starts_with(yours(&team)));
}

#[test]
fn a_previous_path_that_names_no_flow_file_is_not_found() {
    let mut team = Team::new();
    let other = team.config.path().join("precious.toml");
    std::fs::write(&other, "keep me").expect("a file");

    let refused = save_refused(&mut team, &named("Draft"), &json!({"path": other}));

    assert_eq!(refused.code, "not-found");
    assert_eq!(std::fs::read_to_string(&other).expect("the file"), "keep me");
    assert!(files(&yours(&team)).iter().all(|name| !name.starts_with("draft")));
}

#[test]
fn a_draft_for_a_project_that_is_not_open_is_not_found() {
    let mut team = Team::new();

    let refused = save_refused(&mut team, &named("Draft"), &json!({"place": "project", "project": 99999}));

    assert_eq!(refused.code, "not-found");
}

#[test]
fn a_draft_over_the_size_a_flow_can_be_shared_at_is_malformed() {
    let mut team = Team::new();
    let mut draft = named("Huge");
    draft.roles[0].prompt = "x".repeat(share::MAX_BYTES);
    let before = files(&yours(&team));

    let refused = save_refused(&mut team, &draft, &json!({"check": true}));
    let saving = save_refused(&mut team, &draft, &json!({}));

    assert_eq!((refused.code.as_str(), saving.code.as_str()), ("malformed", "malformed"));
    assert!(refused.message.contains("too large"), "{}", refused.message);
    assert_eq!(files(&yours(&team)), before);
}

#[test]
fn an_unknown_place_is_malformed() {
    let mut team = Team::new();

    let refused = save_refused(&mut team, &named("Draft"), &json!({"place": "cloud"}));

    assert_eq!(refused.code, "malformed");
}

#[test]
fn a_project_draft_says_what_env_and_args_it_sets() {
    let mut team = Team::new();
    let mut draft = named("Risky");
    draft.roles[0].env.insert("NODE_ENV".into(), "x".into());
    draft.roles[0].args.push("--verbose".into());

    let checked = saved(&mut team, &draft, &json!({"place": "project", "check": true}));

    let risks = checked.check.expect("the risks");
    assert!(risks.contains("NODE_ENV"), "{risks}");
    assert!(saved(&mut team, &draft, &json!({"check": true})).check.is_none());
}

#[test]
fn the_open_flows_dialog_sees_the_saved_flow() {
    let mut team = Team::new();
    team.app.open_flows_tab();

    let path = saved(&mut team, &named("Fresh"), &json!({})).path.expect("a path");

    let Some(Overlay::Flows { dialog, .. }) = &team.app.overlay else { panic!("the dialog closed") };
    assert!(dialog.find(&Origin::User(path), "").is_some());
}

#[test]
fn yours_and_the_projects_flows_are_deleted() {
    let mut team = Team::new();
    let mine = saved(&mut team, &named("Mine"), &json!({})).path.expect("a path");
    let theirs = saved(&mut team, &named("Theirs"), &json!({"place": "project"})).path.expect("a path");

    let first = removed(ok(&mut team.app, "flow-delete", &json!({"name": "mine"})));
    let second = removed(ok(&mut team.app, "flow-delete", &json!({"name": "Theirs", "place": "project"})));

    assert!(!mine.exists() && !theirs.exists());
    assert_eq!((first.origin.as_deref(), first.deleted, first.changed), (Some("user"), Some(mine), true));
    assert_eq!(second.origin.as_deref(), Some("project"));
    assert!(!listed(&mut team).flows.iter().any(|f| f.name == "Mine" || f.name == "Theirs"));
}

#[test]
fn a_file_that_cannot_be_read_is_deleted_by_its_path() {
    let mut team = Team::new();
    let broken = yours(&team).join("broken.toml");
    std::fs::write(&broken, "not = [a flow").expect("a file");
    assert_eq!(listed(&mut team).broken.len(), 1);

    let gone = removed(ok(&mut team.app, "flow-delete", &json!({"path": broken})));

    assert!(!broken.exists());
    assert_eq!(gone.deleted.as_deref(), Some(broken.as_path()));
}

#[test]
fn a_path_that_is_no_flow_file_is_not_deleted() {
    let mut team = Team::new();
    let other = team.config.path().join("precious.toml");
    std::fs::write(&other, "keep me").expect("a file");

    let refused = refused(&mut team.app, "flow-delete", &json!({"path": other}));

    assert_eq!(refused.code, "not-found");
    assert!(other.exists());
}

#[test]
fn a_built_in_or_your_version_of_one_is_not_deleted_but_named_hide_and_restore() {
    let mut team = Team::new();
    let built = refused(&mut team.app, "flow-delete", &json!({"name": "Quick"}));
    assert_eq!(built.code, "conflict");
    assert!(built.message.contains("flow hide Quick"), "{}", built.message);
    let edit = saved(&mut team, &named("Quick"), &json!({})).path.expect("a path");

    let edited = refused(&mut team.app, "flow-delete", &json!({"name": "Quick"}));
    let by_path = refused(&mut team.app, "flow-delete", &json!({"path": edit}));

    assert_eq!(edited.code, "conflict");
    assert!(edited.message.contains("flow restore Quick") && edited.message.contains("flow hide Quick"));
    assert_eq!(by_path.code, "conflict");
    assert!(edit.exists());
}

#[test]
fn a_deleted_built_in_is_not_deleted_again() {
    let mut team = Team::new();
    ok(&mut team.app, "flow-hide", &json!({"name": "Quick"}));

    let refused = refused(&mut team.app, "flow-delete", &json!({"name": "Quick"}));

    assert_eq!(refused.code, "not-found");
    assert!(refused.message.contains("already deleted"), "{}", refused.message);
}

#[test]
fn an_ambiguous_name_asks_for_its_place() {
    let mut team = Team::new();
    saved(&mut team, &named("Twin"), &json!({}));
    saved(&mut team, &named("Twin"), &json!({"place": "project"}));

    let refused = refused(&mut team.app, "flow-delete", &json!({"name": "Twin"}));

    assert_eq!(refused.code, "conflict");
    assert!(refused.message.contains("--in"), "{}", refused.message);
    removed(ok(&mut team.app, "flow-delete", &json!({"name": "Twin", "place": "yours"})));
    removed(ok(&mut team.app, "flow-delete", &json!({"name": "Twin"})));
}

#[test]
fn a_built_in_is_hidden_and_restored() {
    let mut team = Team::new();

    let hidden = removed(ok(&mut team.app, "flow-hide", &json!({"name": "quick"})));
    let again = removed(ok(&mut team.app, "flow-hide", &json!({"name": "Quick"})));

    assert_eq!((hidden.hidden, hidden.changed, hidden.name.as_str()), (true, true, "Quick"));
    assert!(!again.changed && again.hidden);
    assert_eq!(team.app.config.hidden_flows, ["Quick"]);
    assert!(crate::config::load(&team.config.path().join("config.json")).hidden_flows.contains(&"Quick".to_string()));
    let run = wire::FlowRun { name: "Quick".into(), prompt: "x".into(), ..wire::FlowRun::default() };
    ask(&mut team.app, None, Command::FlowRun(run));
    let refused = error(answered(&mut team.app, &team.rx, "the flow is refused"));
    assert!(refused.contains("was deleted"), "{refused}");

    let back = removed(ok(&mut team.app, "flow-restore", &json!({"name": "Quick"})));
    let nothing = removed(ok(&mut team.app, "flow-restore", &json!({"name": "Quick"})));

    assert!(back.changed && !back.hidden);
    assert!(!nothing.changed);
    assert_eq!(team.app.config.hidden_flows, Vec::<String>::new());
    assert!(listed(&mut team).flows.iter().any(|f| f.name == "Quick"));
}

#[test]
fn hiding_your_version_of_a_built_in_deletes_it_too() {
    let mut team = Team::new();
    let edit = saved(&mut team, &named("Quick"), &json!({})).path.expect("a path");

    let hidden = removed(ok(&mut team.app, "flow-hide", &json!({"name": "Quick"})));

    assert_eq!(hidden.deleted.as_deref(), Some(edit.as_path()));
    assert!(!edit.exists());
    assert_eq!(team.app.config.hidden_flows, ["Quick"]);
}

#[test]
fn restoring_your_version_deletes_it_and_unhides_the_built_in() {
    let mut team = Team::new();
    saved(&mut team, &named("Quick"), &json!({}));
    ok(&mut team.app, "flow-hide", &json!({"name": "Quick"}));
    let again = saved(&mut team, &named("Quick"), &json!({}));
    let edit_again = again.path.expect("a path");

    let back = removed(ok(&mut team.app, "flow-restore", &json!({"name": "Quick"})));

    assert_eq!(back.deleted.as_deref(), Some(edit_again.as_path()));
    assert!(!edit_again.exists());
    assert_eq!(team.app.config.hidden_flows, Vec::<String>::new());
    let flows = listed(&mut team);
    let quick: Vec<&str> = flows.flows.iter().filter(|f| f.name == "Quick").map(|f| f.origin.as_str()).collect();
    assert_eq!(quick, ["built-in"]);
}

#[test]
fn restoring_what_is_shown_and_not_edited_changes_nothing() {
    let mut team = Team::new();
    let revision = team.app.config_file.revision();

    let nothing = removed(ok(&mut team.app, "flow-restore", &json!({"name": "Quick"})));

    assert!(!nothing.changed && nothing.deleted.is_none());
    assert_eq!(team.app.config_file.revision(), revision);
}

#[test]
fn hide_and_restore_name_only_built_ins() {
    let mut team = Team::new();

    let hide = refused(&mut team.app, "flow-hide", &json!({"name": "Pair"}));
    let restore = refused(&mut team.app, "flow-restore", &json!({"name": "Nothing"}));

    assert_eq!((hide.code.as_str(), restore.code.as_str()), ("not-found", "not-found"));
    assert!(hide.message.contains("Quick"), "{}", hide.message);
}

#[test]
fn a_running_flow_keeps_its_plan_when_its_flow_is_deleted() {
    let mut team = Team::new();
    let run = team.started();

    removed(ok(&mut team.app, "flow-delete", &json!({"name": "Pair"})));

    assert_eq!(files(&yours(&team)), Vec::<String>::new());
    assert_eq!(team.app.runs.all().len(), 1);
    assert_eq!(team.app.runs.all()[0].flow.name, "Pair");
    assert_eq!(team.app.runs.all()[0].workspace, run.workspace);
}
