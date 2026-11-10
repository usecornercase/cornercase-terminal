use std::path::PathBuf;

use super::*;
use crate::agents::Known;
use crate::flows::builtin;

fn valid(name: &str) -> Flow {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.name = name.into();
    flow
}

fn yours(name: &str) -> Listing {
    Listing {
        flows: vec![Saved { flow: valid(name), origin: Origin::User(PathBuf::from("pair.toml")) }],
        ..Listing::default()
    }
}

#[test]
fn a_flows_own_problems_come_first_and_a_clash_after_them() {
    let mut draft = valid("Pair");
    draft.stages.clear();
    let origin = Origin::Project(PathBuf::from("p/pair.toml"));

    let (problems, own) = save_problems(&draft, &yours("Pair"), &origin, None);

    assert!(own > 0);
    assert_eq!(problems[..own], draft.problems());
    assert_eq!(problems[own..], ["yours is also named `Pair` and runs instead"]);
}

#[test]
fn a_clash_alone_is_no_problem_of_its_own() {
    let origin = Origin::Project(PathBuf::from("p/pair.toml"));

    let (problems, own) = save_problems(&valid("Pair"), &yours("Pair"), &origin, None);

    assert_eq!((problems.len(), own), (1, 0));
}

#[test]
fn a_flow_with_no_namesake_has_only_its_own_problems() {
    let mut draft = valid("Pair");
    draft.stages.clear();
    let origin = Origin::Project(PathBuf::from("p/pair.toml"));

    let (problems, own) = save_problems(&draft, &Listing::default(), &origin, None);

    assert!(own > 0);
    assert_eq!(own, problems.len());
}
