use std::path::PathBuf;

use super::said::{Outcome, TreeFreed, builds_said};
use crate::app::closed_agents::{closed_and, closed_message};
use crate::app::control::worktrees::Risky;
use crate::control::BuildsFreed;
use crate::control::worktrees::FailedFolder;
use crate::worktrees::builds::{Folder, Size};

const GB: u64 = 1024 * 1024 * 1024;

fn path() -> PathBuf {
    PathBuf::from("wt")
}

fn freed(bytes: u64) -> Outcome {
    Outcome::Freed(BuildsFreed { bytes, ..BuildsFreed::default() })
}

fn kept(running: &[&str], builds: u64, nested: &[&str]) -> Outcome {
    let folder = Folder { path: path().join("target"), bytes: builds };
    let size = Size {
        bytes: builds,
        builds: vec![folder],
        nested: nested.iter().map(|n| path().join(n).join(".git")).collect(),
    };
    Outcome::Kept(Risky { running: running.iter().map(ToString::to_string).collect(), size })
}

fn tree(role: Option<&str>, outcome: Outcome) -> TreeFreed {
    TreeFreed { role: role.map(String::from), path: path(), outcome }
}

mod what_it_says {
    use super::*;

    #[test]
    fn one_freed_tree_says_how_much() {
        let (said, failed) = builds_said(&[tree(None, freed(GB * 157 / 10))]);
        assert_eq!(said, "15.7 GB of builds freed");
        assert!(!failed);
    }

    #[test]
    fn nothing_anywhere_says_there_were_no_builds() {
        let trees = [tree(None, freed(0)), tree(Some("coder A"), kept(&["cargo"], 0, &[]))];
        assert_eq!(builds_said(&trees), ("no builds to free".to_string(), false));
    }

    #[test]
    fn a_kept_role_is_named_next_to_what_was_freed() {
        let trees = [tree(None, freed(GB * 98 / 10)), tree(Some("coder A"), kept(&["cargo"], GB * 21 / 10, &[]))];
        let (said, failed) = builds_said(&trees);
        assert_eq!(said, "9.8 GB of builds freed; coder A's worktree kept 2.1 GB of builds: cargo runs in it");
        assert!(!failed);
    }

    #[test]
    fn several_programs_run_in_it() {
        let (said, _) = builds_said(&[tree(None, kept(&["cargo", "node"], GB, &[]))]);
        assert_eq!(said, "the worktree kept 1.0 GB of builds: cargo, node run in it");
    }

    #[test]
    fn a_checkout_inside_is_named_relative_to_the_worktree() {
        let (said, _) = builds_said(&[tree(None, kept(&[], GB, &[".venv/src/foo"]))]);
        assert_eq!(said, "the worktree kept 1.0 GB of builds: .venv/src/foo is a git checkout");
    }

    #[test]
    fn a_folder_that_would_not_go_is_a_failure() {
        let failed = BuildsFreed {
            bytes: 0,
            failed: vec![
                FailedFolder { path: "target".into(), error: "busy".into() },
                FailedFolder { path: "dist".into(), error: "busy".into() },
            ],
            ..BuildsFreed::default()
        };
        let (said, failed) = builds_said(&[tree(None, Outcome::Freed(failed))]);
        assert_eq!(said, "could not free the worktree: target: busy and 1 more");
        assert!(failed);
    }

    #[test]
    fn a_folder_kept_at_deletion_is_named() {
        let kept = BuildsFreed { kept: vec!["target".into()], ..BuildsFreed::default() };
        let (said, failed) = builds_said(&[tree(None, Outcome::Freed(kept))]);
        assert_eq!(said, "the worktree kept target: no longer an ignored folder there, or on another filesystem");
        assert!(!failed);
    }

    #[test]
    fn a_refused_tree_names_its_role_and_the_reason() {
        let (said, failed) = builds_said(&[tree(Some("coder A"), Outcome::Failed("it is being removed".into()))]);
        assert_eq!(said, "could not free coder A's worktree: it is being removed");
        assert!(failed);
    }
}

mod the_closing_text {
    use super::*;

    #[test]
    fn it_adds_the_clause_before_the_reopen_hint() {
        let said = closed_and(5, Some("15.7 GB of builds freed"));
        assert_eq!(said, "5 agents closed, 15.7 GB of builds freed · ▶ on the workspace reopens them");
    }

    #[test]
    fn without_a_clause_it_is_the_plain_message() {
        assert_eq!(closed_and(1, None), closed_message(1));
        assert_eq!(closed_message(1), "1 agent closed · ▶ on the workspace reopens it");
    }
}
