use super::*;
use crate::flows::persona;

const REVIEWER: &str = "---\nname: Code Reviewer\n---\n\nYou review code like a mentor.\n";

fn with_a_persona(team: &Team) -> PathBuf {
    let config = team.config.path().join("config.json");
    let mut flow = pair();
    flow.name = "Reviewed".into();
    flow.roles[1].persona = Some("engineering-code-reviewer".into());
    store::save(&paths::flows(&config), &flow, None).expect("save the flow");
    let dir = paths::personas(&config);
    persona::save(&dir, &persona::parse("engineering-code-reviewer", REVIEWER).expect("a persona")).expect("saved");
    dir
}

fn started(team: &mut Team) -> RunInfo {
    match team.run("Reviewed", None) {
        Response::Ok(value) => serde_json::from_value(value).expect("the run"),
        Response::Error(message) => panic!("the flow did not start: {message}"),
    }
}

#[test]
fn a_role_reads_its_persona_from_the_run_folder_where_the_run_keeps_a_copy() {
    let mut team = Team::new();
    let library = with_a_persona(&team);
    let run = started(&mut team);

    let order = team.until_got(3);

    let dir = run.run_dir.expect("the run's folder");
    let copy = dir.join("personas").join("engineering-code-reviewer.md");
    assert_eq!(std::fs::read_to_string(&copy).expect("the copy"), REVIEWER);
    let reviewer = &order[position(&order, "got Review it.")];
    assert!(reviewer.contains(&format!("read the persona `{}`", copy.display())), "{reviewer}");
    let coder = &order[position(&order, "got Write it.")];
    assert!(!coder.contains("persona"), "{coder}");
    let brief = &order[position(&order, "got You are the orchestrator")];
    assert!(brief.contains("follows the persona engineering-code-reviewer"), "{brief}");

    persona::remove(&library, "engineering-code-reviewer").expect("removed");
    std::fs::remove_dir_all(dir.join("personas")).expect("the copy goes");
    let kept = team.app.runs.all()[0].clone();
    assert!(team.app.reopen_run_dir(&kept).is_some());
    assert_eq!(std::fs::read_to_string(&copy).expect("written again"), REVIEWER);
    let state = team.app.state();
    let saved = state.projects.iter().flat_map(|p| &p.workspaces).find_map(|w| w.run.as_ref()).expect("a run");
    assert_eq!(saved.personas.get("engineering-code-reviewer").map(String::as_str), Some(REVIEWER));
}

#[test]
fn a_run_whose_persona_is_gone_does_not_start() {
    let mut team = Team::new();
    let library = with_a_persona(&team);
    persona::remove(&library, "engineering-code-reviewer").expect("removed");

    let refused = team.run("Reviewed", None);

    assert!(
        matches!(&refused, Response::Error(message) if message.contains("follows the persona `engineering-code-reviewer`, which is not among your personas")),
        "{refused:?}"
    );
    assert!(team.app.runs.all().is_empty());
}
