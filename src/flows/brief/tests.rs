use std::path::Path;

use super::{Closed, Team, brief, member, reopened, restored, where_it_stands};
use crate::agents::Known;
use crate::flows::run::{Member, RoleWorktree};
use crate::flows::{Back, End, Flow, ORCHESTRATOR, Role, builtin, role_branch};

fn members() -> Vec<Member> {
    [(ORCHESTRATOR, 12), ("coder", 14), ("reviewer", 15), ("security", 16), ("tester", 17)]
        .into_iter()
        .map(|(role, pane)| Member { role: role.into(), pane })
        .collect()
}

fn assert_holds(text: &str, needles: &[&str]) {
    let missing: Vec<&&str> = needles.iter().filter(|needle| !text.contains(**needle)).collect();
    assert!(missing.is_empty(), "missing {missing:?}: {text}");
}

#[test]
fn the_brief_holds_the_task_the_team_the_plan_and_the_end() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.roles[1].model = Some("gpt-5.5".into());
    flow.roles[1].effort = Some("high".into());
    flow.notes = "Do not change the public API.".into();
    let text = brief(&flow, "Fix the login form\n", "fix-login", Team { members: &members(), trees: &[] }, None);

    assert_holds(
        &text,
        &[
            "flow \"Full issue\"",
            "on branch fix-login",
            "## Task\n\nFix the login form\n\n## Team",
            "- coder (pane 14): claude, edits files\n",
            "- reviewer (pane 15): codex · gpt-5.5 · high, must not edit files\n",
            "- you, the orchestrator (pane 12): claude\n",
            "1. coder.\n",
            "2. reviewer and security, in parallel. If either reports findings, send them to coder and go back to \
             stage 1; at most 3 times,",
            "3. tester. If it reports failures, send them to coder and go back to stage 1; at most 2",
            "At the end: push branch fix-login and open a pull request",
            "(a merge request with `glab mr create` when the repository is on GitLab)",
            "Do not change the public API.\n\nKeep every message",
            "`cornercase wait --all --pane 14 --pane 15 --pane 16 --pane 17 --until turn-over",
            "run `cornercase mark --pane 12`",
            "--until turn-over --timeout 300` (several agents",
        ],
    );
    assert_eq!(text.matches("--timeout 300`").count(), 2, "one time limit for every wait: {text}");
    assert!(text.contains("Give your shell tool a time limit longer than `--timeout` when it takes one"), "{text}");
    assert!(text.contains("Never wait in the background"), "{text}");
    assert!(!text.contains("--pane 12 --until"), "{text}");
}

#[test]
fn an_answer_whose_record_cannot_be_found_is_read_from_the_screen() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "Fix the login form", "fix-login", Team { members: &members(), trees: &[] }, None);
    assert!(
        text.contains(
            "If it fails for an agent (its record cannot be found), read its screen with `cornercase read --pane N \
             --lines 300` and take the answer from there."
        ),
        "{text}"
    );
}

#[test]
fn a_flow_that_leaves_the_commits_says_not_to_push() {
    let mut flow = builtin::flows(&[Known::Codex], None).remove(2);
    flow.end = End::Commits;
    flow.stages[1].back = Some(Back { to: 1, when: "findings".into(), at_most: 1 });
    let text = brief(&flow, "x", "quick", Team { members: &members(), trees: &[] }, None);

    assert!(text.contains("If it reports findings, send them to coder and go back to stage 1; at most once"));
    assert!(text.contains("At the end: leave the commits on branch quick; do not push"), "{text}");
}

#[test]
fn three_roles_in_parallel_are_any_of_them() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.stages[1].roles.push("tester".into());
    let text = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);

    assert!(text.contains("2. reviewer, security and tester, in parallel. If any of them reports findings"));
}

#[test]
fn a_member_is_told_whether_it_may_edit() {
    let coder = Role { name: "coder".into(), edits: true, prompt: "Write code.".into(), ..Role::default() };
    assert_eq!(
        member(&coder, None, None),
        "Write code.\n\nYou are `coder` in this team, and you may edit files and commit in this worktree. Wait for \
         the orchestrator's first message before doing anything."
    );
    let empty = Role { name: "reviewer".into(), ..Role::default() };
    assert_eq!(
        member(&empty, None, None),
        "You are `reviewer` in this team: do not edit the worktree's files and do not commit. Wait for the orchestrator's first \
         message before doing anything."
    );
}

fn arena_trees() -> (Flow, Vec<RoleWorktree>) {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).pop().expect("arena");
    let trees = flow
        .roles
        .iter()
        .filter(|role| role.own_worktree())
        .map(|role| RoleWorktree {
            role: role.name.clone(),
            path: format!("/wt/{}", role.name.replace(' ', "-")).into(),
            branch: role_branch("fix", &role.name),
        })
        .collect();
    (flow, trees)
}

fn arena_members() -> Vec<Member> {
    std::iter::once(ORCHESTRATOR)
        .chain(["coder A", "coder B", "coder C", "judge", "integrator", "reviewer"])
        .zip(12..)
        .map(|(role, pane)| Member { role: role.into(), pane })
        .collect()
}

#[test]
fn the_team_line_of_a_role_with_its_own_worktree_names_its_folder_and_branch() {
    let (flow, trees) = arena_trees();
    let text = brief(&flow, "x", "fix", Team { members: &arena_members(), trees: &trees }, None);

    assert!(
        text.contains(
            "- coder A (pane 13): claude, edits files, in its own worktree /wt/coder-A on branch fix--coder-a\n"
        ),
        "{text}"
    );
    assert!(text.contains("- judge (pane 16): claude, must not edit files\n"), "{text}");
}

#[test]
fn the_worktrees_section_tells_how_to_read_and_integrate_candidates() {
    let (flow, trees) = arena_trees();
    let text = brief(&flow, "x", "fix", Team { members: &arena_members(), trees: &trees }, None);

    assert!(text.contains("## Worktrees"), "{text}");
    assert!(text.contains("`git log fix..B`, `git diff fix...B` and `git show B:path`"), "{text}");
    assert!(text.contains("ask a role that edits this worktree to `git merge B`"), "{text}");
    assert!(text.contains("Never check out a role's branch here"), "{text}");
    let plain = brief(&sample_flow(), "x", "fix", Team { members: &members(), trees: &[] }, None);
    assert!(!plain.contains("## Worktrees"), "{plain}");
}

fn sample_flow() -> Flow {
    builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0)
}

#[test]
fn a_member_with_its_own_worktree_is_told_its_branch() {
    let coder = Role { name: "coder A".into(), edits: true, ..Role::default() };
    let told = member(&coder, Some("fix--coder-a"), None);
    assert!(
        told.starts_with(
            "You are `coder A` in this team, and you may edit files and commit in this worktree, which is yours alone, on branch fix--coder-a: commit your work there"
        ),
        "{told}"
    );
    let reader = Role { name: "judge".into(), ..Role::default() };
    assert!(
        member(&reader, Some("b"), None)
            .contains("do not edit the worktree's files and do not commit. This worktree is yours alone, on branch b.")
    );
}

#[test]
fn a_built_in_member_is_told_to_wait_once_and_last() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = member(&flow.roles[1], None, None);

    assert_eq!(text.matches("Wait for the orchestrator's first message").count(), 1, "{text}");
    assert!(
        text.ends_with("do not commit. Wait for the orchestrator's first message before doing anything."),
        "{text}"
    );
}

const DIR: &str = "/s/runs/2026-10-17-api";

fn folder() -> &'static Path {
    Path::new(DIR)
}

#[test]
fn the_brief_names_the_run_folder_and_says_where_it_writes() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, Some(folder()));

    assert!(
        text.contains(&format!("## Files\n\nThe run has a folder for its files, {DIR} (also `$CORNERCASE_RUN_DIR`"))
    );
    assert!(text.contains("`orchestrator-<n>.md`"), "{text}");
    assert!(text.contains("Never edit the worktree's files, commit"), "{text}");
    assert!(text.contains("The run's folder is the only place you write."), "{text}");
    assert!(text.find("## Files").expect("files") < text.find("## How to drive").expect("rules"));
    assert_eq!(text.matches("--timeout 300`").count(), 2, "{text}");
}

#[test]
fn without_a_folder_the_brief_has_no_files_section() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);

    assert!(!text.contains("## Files"), "{text}");
    assert!(!text.contains("only place you write"), "{text}");
    assert!(text.contains("Never edit files, commit"), "{text}");
}

#[test]
fn a_member_with_a_folder_writes_a_file_per_answer_and_still_waits_last() {
    let coder = Role { name: "coder".into(), edits: true, prompt: "Write code.".into(), ..Role::default() };
    let text = member(&coder, None, Some(folder()));
    assert_eq!(
        text,
        format!(
            "Write code.\n\nYou are `coder` in this team, and you may edit files and commit in this worktree. Write \
             each answer to a file of its own in the run's folder, {DIR} (also `$CORNERCASE_RUN_DIR`), named after \
             your role and the next number free there (`coder-1.md`, then `coder-2.md`), and end your turn with that \
             file's path and a summary of a few lines. Anything else too long for a message (a design, logs, a \
             script that shows a bug) goes there too, never in the worktree; if your file tools cannot reach the \
             folder, use your shell. Wait for the orchestrator's first message before doing anything."
        )
    );
    let reviewer = Role { name: "second reviewer".into(), ..Role::default() };
    let text = member(&reviewer, None, Some(folder()));
    assert!(text.contains("do not edit the worktree's files and do not commit. Write each answer"), "{text}");
    assert!(text.contains("`second-reviewer-1.md`, then `second-reviewer-2.md`"), "{text}");
    assert_eq!(text.matches("Wait for the orchestrator's first message").count(), 1, "{text}");
    assert!(text.ends_with("before doing anything."), "{text}");
}

fn names(roles: &[&str]) -> Vec<String> {
    roles.iter().map(ToString::to_string).collect()
}

#[test]
fn a_reopened_brief_names_the_new_panes_the_folder_and_how_to_check_the_team() {
    let closed = Closed { cut: &names(&["coder", "reviewer"]), fresh: &names(&["tester"]) };

    let text = reopened(&members(), Some(folder()), &closed);

    assert_holds(
        &text,
        &[
            "Your team was closed and reopened, so it has new pane ids: coder is pane 14, reviewer is pane 15, \
             security is pane 16, tester is pane 17, and you are pane 12.",
            &format!("The run's folder is still {DIR}."),
            "coder and reviewer were cut in the middle of their turn.",
            "tester starts over in a new conversation with only its role.",
            "`cornercase read --last-message`",
            "If the plan was already done, answer `ok`",
        ],
    );
}

#[test]
fn a_reopened_brief_has_the_singular_forms_and_leaves_out_what_did_not_happen() {
    let closed = Closed { cut: &names(&["coder"]), fresh: &[] };

    let text = reopened(&members(), None, &closed);

    assert!(text.contains(" coder was cut in the middle of its turn."), "{text}");
    assert!(!text.contains("starts over") && !text.contains("folder"), "{text}");
    assert!(!reopened(&members(), None, &Closed { cut: &[], fresh: &[] }).contains("cut in the middle"));
}

#[test]
fn a_fresh_orchestrator_is_told_where_the_run_stands() {
    let closed = Closed { cut: &[], fresh: &names(&["coder", "tester"]) };

    let text = where_it_stands(&members(), Some(folder()), &closed);

    assert!(text.starts_with("\n## Where the run stands\n"), "{text}");
    assert!(text.contains("coder and tester start over in a new conversation with only their role."), "{text}");
}

#[test]
fn a_restored_brief_names_the_folder() {
    let text = restored(&members(), Some(folder()));
    assert!(text.contains(&format!("Use these from now on. The run's folder is still {DIR}. Any wait")), "{text}");
    assert!(!restored(&members(), None).contains("folder"));
}
