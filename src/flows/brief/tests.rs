use super::{brief, member};
use crate::agents::Known;
use crate::flows::run::Member;
use crate::flows::{Back, End, ORCHESTRATOR, Role, builtin};

fn members() -> Vec<Member> {
    [(ORCHESTRATOR, 12), ("coder", 14), ("reviewer", 15), ("security", 16), ("tester", 17)]
        .into_iter()
        .map(|(role, pane)| Member { role: role.into(), pane })
        .collect()
}

#[test]
fn the_brief_holds_the_task_the_team_the_plan_and_the_end() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.roles[1].model = Some("gpt-5.5".into());
    flow.roles[1].effort = Some("high".into());
    flow.notes = "Do not change the public API.".into();
    let text = brief(&flow, "Fix the login form\n", "fix-login", &members());

    assert!(text.contains("flow \"Full issue\""), "{text}");
    assert!(text.contains("on branch fix-login"), "{text}");
    assert!(text.contains("## Task\n\nFix the login form\n\n## Team"), "{text}");
    assert!(text.contains("- coder (pane 14): claude, edits files\n"), "{text}");
    assert!(text.contains("- reviewer (pane 15): codex · gpt-5.5 · high, must not edit files\n"), "{text}");
    assert!(text.contains("- you, the orchestrator (pane 12): claude\n"), "{text}");
    assert!(text.contains("1. coder.\n"), "{text}");
    assert!(
        text.contains(
            "2. reviewer and security, in parallel. If either reports findings, send them to coder and go back to \
             stage 1; at most 3 times,"
        ),
        "{text}"
    );
    assert!(text.contains("3. tester. If it reports failures, send them to coder and go back to stage 1; at most 2"));
    assert!(text.contains("At the end: push branch fix-login and open a pull request"), "{text}");
    assert!(text.contains("Do not change the public API.\n\nKeep every message"), "{text}");
    assert!(text.contains("`cornercase wait --all --pane 14 --pane 15 --pane 16 --pane 17 --until turn-over"));
    assert!(text.contains("run `cornercase mark --pane 12`"), "{text}");
    assert!(text.contains("--until turn-over --timeout 300` (several agents"), "{text}");
    assert_eq!(text.matches("--timeout 300`").count(), 2, "one time limit for every wait: {text}");
    assert!(text.contains("Give your shell tool a time limit longer than `--timeout` when it takes one"), "{text}");
    assert!(text.contains("Never wait in the background"), "{text}");
    assert!(!text.contains("--pane 12 --until"), "{text}");
}

#[test]
fn a_flow_that_leaves_the_commits_says_not_to_push() {
    let mut flow = builtin::flows(&[Known::Codex], None).remove(2);
    flow.end = End::Commits;
    flow.stages[1].back = Some(Back { to: 1, when: "findings".into(), at_most: 1 });
    let text = brief(&flow, "x", "quick", &members());

    assert!(text.contains("If it reports findings, send them to coder and go back to stage 1; at most once"));
    assert!(text.contains("At the end: leave the commits on branch quick; do not push"), "{text}");
}

#[test]
fn three_roles_in_parallel_are_any_of_them() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.stages[1].roles.push("tester".into());
    let text = brief(&flow, "x", "b", &members());

    assert!(text.contains("2. reviewer, security and tester, in parallel. If any of them reports findings"));
}

#[test]
fn a_member_is_told_whether_it_may_edit() {
    let coder = Role { name: "coder".into(), edits: true, prompt: "Write code.".into(), ..Role::default() };
    assert_eq!(
        member(&coder),
        "Write code.\n\nYou are `coder` in this team, and you may edit files and commit in this worktree. Wait for \
         the orchestrator's first message before doing anything."
    );
    let empty = Role { name: "reviewer".into(), ..Role::default() };
    assert_eq!(
        member(&empty),
        "You are `reviewer` in this team: do not edit files and do not commit. Wait for the orchestrator's first \
         message before doing anything."
    );
}

#[test]
fn a_built_in_member_is_told_to_wait_once_and_last() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = member(&flow.roles[1]);

    assert_eq!(text.matches("Wait for the orchestrator's first message").count(), 1, "{text}");
    assert!(
        text.ends_with("do not commit. Wait for the orchestrator's first message before doing anything."),
        "{text}"
    );
}
