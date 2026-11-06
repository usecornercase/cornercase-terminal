use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::test_util::fixture;

fn imported(projects: &[u64], missing: usize, failed: usize) -> ImportedWorkspace {
    ImportedWorkspace {
        group: Some(5),
        name: "team".into(),
        projects: projects.to_vec(),
        missing,
        failed,
        projection: None,
    }
}

mod the_answer {
    use super::*;

    #[test]
    fn is_written_as_its_fixture() {
        let answer = ImportedWorkspace { projection: Some(3), ..imported(&[6, 7], 1, 0) };

        let written = serde_json::to_string_pretty(&answer).expect("json");

        assert_eq!(written, fixture!("control/open/imported.json").trim_end());
    }

    #[test]
    fn reads_its_fixture_back() {
        let read: ImportedWorkspace = serde_json::from_str(fixture!("control/open/imported.json")).expect("an answer");

        assert_eq!(read, ImportedWorkspace { projection: Some(3), ..imported(&[6, 7], 1, 0) });
    }

    #[test]
    fn says_there_is_no_group_with_a_null_and_leaves_the_projection_out_without_a_view() {
        let answer = ImportedWorkspace { group: None, ..imported(&[], 2, 0) };

        let value = serde_json::to_value(&answer).expect("json");

        assert_eq!(value, json!({"group": null, "name": "team", "projects": [], "missing": 2, "failed": 0}));
    }

    #[test]
    fn from_a_newer_server_reads_with_its_extra_keys() {
        let newer = json!({"group": 5, "projects": [6], "missing": 0, "failed": 0, "newer": true});

        let read: ImportedWorkspace = serde_json::from_value(newer).expect("an answer");

        assert_eq!((read.group, read.projects), (Some(5), vec![6]));
    }

    #[test]
    fn can_be_told_from_a_folders_by_its_projects() {
        let folder: Value = serde_json::to_value(crate::control::Done::default()).expect("json");
        let file: Value = serde_json::to_value(ImportedWorkspace::default()).expect("json");

        assert_eq!((folder.get("projects").is_some(), file.get("projects").is_some()), (false, true));
    }
}

#[rstest]
#[case::one(&[6], 0, 0, "1 project imported into team", true)]
#[case::several(&[6, 7], 0, 0, "2 projects imported into team", true)]
#[case::one_gone(&[6], 1, 0, "1 project imported into team, 1 not found", false)]
#[case::one_failed(&[6, 7], 0, 1, "2 projects imported into team, 1 could not open, see server.log", false)]
#[case::both(&[6], 1, 2, "1 project imported into team, 1 not found, 2 could not open, see server.log", false)]
#[case::none_found(&[], 1, 0, "nothing imported from team, 1 not found", false)]
#[case::no_folders(&[], 0, 0, "nothing imported from team", false)]
fn it_says_what_the_toast_says_and_whether_every_folder_opened(
    #[case] projects: &[u64],
    #[case] missing: usize,
    #[case] failed: usize,
    #[case] message: &str,
    #[case] whole: bool,
) {
    let answer = imported(projects, missing, failed);

    assert_eq!((answer.message().as_str(), answer.whole()), (message, whole));
}
