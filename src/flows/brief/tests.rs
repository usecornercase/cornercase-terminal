use std::path::Path;

use super::{Closed, Team, brief, failed, finished, member, reopened, restored, where_it_stands, woken};
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
            "- coder (pane 14): claude, edits files, compacts with `/compact` and what it must keep\n",
            "- reviewer (pane 15): codex · gpt-5.5 · high, must not edit files, compacts with `/compact` alone\n",
            "- you, the orchestrator (pane 12): claude\n",
            "1. coder.\n",
            "2. reviewer and security, in parallel. If either reports findings, send them to coder and go back to \
             stage 1, then run every stage after it again, in order. Do this at most 3 times; after that, go on \
             even if either reports findings again, and list what is left in your final summary.",
            "3. tester. If it reports failures, send them to coder and go back to stage 1, then run every stage \
             after it again, in order. Do this at most 2 times; after that, go on even if it reports failures again",
            "At the end, once every stage after the last change has run on it, and each of them that has a loop \
             answered clean or reached its limit (then list what is left): push branch fix-login and open a pull request",
            "(a merge request with `glab mr create` when the repository is on GitLab)",
            "Do not change the public API.\n\nKeep every message",
            "`cornercase wait --all --pane 14 --pane 15 --pane 16 --pane 17 --until turn-over",
            "run `cornercase mark --pane 12`",
            "turn-over --timeout 540`. Run one wait at a time, with a `--pane` for each agent you wait for",
            "never several identical waits",
            "(600000 ms on Claude Code's Bash",
            "If the user writes to you while you wait, answer, then wait again.",
            "These rules hold after the plan too",
        ],
    );
    assert_eq!(text.matches("--timeout 540`").count(), 3, "one time limit for every wait and the compact: {text}");
    assert!(!text.contains("--timeout 300"), "{text}");
    assert!(text.contains("Never wait in the background"), "{text}");
    assert!(!text.contains("--pane 12 --until"), "{text}");
}

#[test]
fn the_brief_tells_how_to_watch_and_compact_the_team() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "x", "fix", Team { members: &members(), trees: &[] }, None);

    assert_holds(
        &text,
        &[
            "`cornercase status --panes | cut -f 1,7,12,13,26,32`",
            "Above about 70 %",
            "`cornercase send --pane N --enter '/compact …' --timeout 540`",
            "compacted is true until its next reply",
            "You cannot compact yourself: when your own context passes about 80 %, tell the user with \
             `cornercase notify`",
        ],
    );
    assert!(text.find("| cut -f").expect("rule") < text.find("- Never edit").expect("next rule"), "{text}");
}

#[test]
fn a_role_whose_agent_has_no_compact_command_gets_none() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.roles[0].agent = "aider".into();
    let text = brief(&flow, "x", "fix", Team { members: &members(), trees: &[] }, None);

    let line = text.lines().find(|line| line.starts_with("- coder")).expect("the coder's line");
    assert!(!line.contains("compacts"), "{line}");
}

#[rstest::rstest]
#[case::gemini("gemini", "compacts with `/compress` alone")]
#[case::grok("grok", "compacts with `/compact` alone")]
#[case::cursor("cursor", "compacts with `/summarize` alone")]
#[case::copilot("copilot", "compacts with `/compact` and what it must keep")]
fn each_agent_is_told_its_own_compact_command(#[case] agent: &str, #[case] said: &str) {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.roles[0].agent = agent.into();
    let text = brief(&flow, "x", "fix", Team { members: &members(), trees: &[] }, None);

    let line = text.lines().find(|line| line.starts_with("- coder")).expect("the coder's line");
    assert!(line.contains(said), "{line}");
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

    assert!(
        text.contains(
            "send them to coder and go back to stage 1, then run every stage after it again, in order. Do this at \
             most once;"
        ),
        "{text}"
    );
    assert!(text.contains("(then list what is left): leave the commits on branch quick; do not push"), "{text}");
}

#[test]
fn a_loop_runs_its_stage_and_every_stage_after_it_again() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);

    assert!(
        text.contains(
            "When the work goes back to an earlier stage, that stage and every stage after it run again, in order, \
             on what changed, however small the change"
        ),
        "{text}"
    );
    assert_eq!(text.matches("then run every stage after it again, in order").count(), 2, "{text}");
    assert!(!text.contains("then go on"), "{text}");
}

#[test]
fn a_last_stage_without_a_loop_does_not_hold_the_final_action_for_a_clean_answer() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.stages[2].back = None;
    let text = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);

    assert!(text.contains("and each of them that has a loop answered clean or reached its limit"), "{text}");
    assert!(!text.contains("its last answer is clean"), "{text}");
}

#[test]
fn the_final_action_waits_for_every_stage_after_the_last_change() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let when = "At the end, once every stage after the last change has run on it, and each of them that has a loop \
                answered clean or reached its limit (then list what is left): ";
    let pull = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);
    assert!(pull.contains(&format!("{when}push branch b")), "{pull}");
    flow.end = End::Commits;
    let commits = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);
    assert!(commits.contains(&format!("{when}leave the commits on branch b")), "{commits}");
}

#[test]
fn three_roles_in_parallel_are_any_of_them() {
    let mut flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    flow.stages[1].roles.push("tester".into());
    let text = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);

    assert!(text.contains("2. reviewer, security and tester, in parallel. If any of them reports findings"));
    assert!(text.contains("go on even if any of them reports findings again"), "{text}");
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
    assert!(!member(&coder, None, None).contains("task.md"));
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
            "- coder A (pane 13): claude, edits files, in its own worktree /wt/coder-A on branch fix--coder-a, compacts with `/compact` and what it must keep\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "- judge (pane 16): claude, must not edit files, compacts with `/compact` and what it must keep\n"
        ),
        "{text}"
    );
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
    let task = folder().join("task.md").display().to_string();
    assert!(
        text.contains(&format!(
            "The task is in it as `task.md` ({task}), exactly as it was given, and every role's prompt names that file."
        )),
        "{text}"
    );
    assert_holds(
        &text,
        &[
            "send each of its roles the path of `task.md` (see Files) and what this stage adds",
            "never a retelling of the task",
            "## Task\n\nx\n",
        ],
    );
    assert!(text.contains("`orchestrator-<n>.md`"), "{text}");
    assert!(text.contains("Never edit the worktree's files, commit"), "{text}");
    assert!(text.contains("The run's folder is the only place you write."), "{text}");
    assert!(text.find("## Files").expect("files") < text.find("## How to drive").expect("rules"));
    assert_eq!(text.matches("--timeout 540`").count(), 3, "{text}");
}

#[test]
fn without_a_folder_the_brief_has_no_files_section() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "x", "b", Team { members: &members(), trees: &[] }, None);

    assert!(!text.contains("## Files"), "{text}");
    assert!(!text.contains("task.md"), "{text}");
    assert!(text.contains("send each of its roles what it needs (the task, what the stages before produced)"));
    assert!(!text.contains("only place you write"), "{text}");
    assert!(text.contains("Never edit files, commit"), "{text}");
}

#[test]
fn a_member_with_a_folder_writes_a_file_per_answer_and_still_waits_last() {
    let coder = Role { name: "coder".into(), edits: true, prompt: "Write code.".into(), ..Role::default() };
    let text = member(&coder, None, Some(folder()));
    let task = folder().join("task.md").display().to_string();
    assert_eq!(
        text,
        format!(
            "Write code.\n\nYou are `coder` in this team, and you may edit files and commit in this worktree. The \
             run's folder is {DIR} (also `$CORNERCASE_RUN_DIR`). The task in full, exactly as it was given, is \
             `task.md` there ({task}): read it before you start on the work you are sent, since the orchestrator's \
             messages say only what each step adds. Write each answer to a file of its own in that folder, named \
             after your role and the next number free there (`coder-1.md`, then `coder-2.md`), and end your turn \
             with that file's path and a summary of a few lines. Anything else too long for a message (a design, \
             logs, a script that shows a bug) goes there too, never in the worktree; if your file tools cannot \
             reach the folder, use your shell. Wait for the orchestrator's first message before doing anything."
        )
    );
    let reviewer = Role { name: "second reviewer".into(), ..Role::default() };
    let text = member(&reviewer, None, Some(folder()));
    assert!(text.contains("do not edit the worktree's files and do not commit. The run's folder is"), "{text}");
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

#[test]
fn a_finished_role_is_named_with_how_to_read_its_answer() {
    let text = finished(&[("coder".into(), 113)]);
    assert_eq!(
        text,
        "coder (pane 113) finished the work you sent it. Read its answer with `cornercase read --last-message \
         --pane 113`, then go on."
    );
}

#[test]
fn several_finished_roles_share_one_message() {
    let two = finished(&[("coder".into(), 14), ("reviewer".into(), 15)]);
    assert!(two.starts_with("coder (pane 14) and reviewer (pane 15) finished the work you sent them."), "{two}");
    assert!(two.contains("Read their answers with `cornercase read --last-message --pane 14 --pane 15`"), "{two}");
    let three = finished(&[("a".into(), 1), ("b".into(), 2), ("c".into(), 3)]);
    assert!(three.starts_with("a (pane 1), b (pane 2) and c (pane 3) finished"), "{three}");
}

#[test]
fn the_plan_names_the_small_changes_that_run_every_later_stage_again() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "x", "fix", Team { members: &members(), trees: &[] }, None);

    assert_holds(
        &text,
        &["however small the change (a fix of docs only, a rebase or a merge of a moved branch included)"],
    );
}

#[test]
fn the_rules_say_what_to_do_with_a_failed_agent_and_a_skipped_stage() {
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let text = brief(&flow, "x", "fix", Team { members: &members(), trees: &[] }, None);

    assert_holds(
        &text,
        &[
            "An agent whose turn ended `failed` (its last message is an `API Error` line, for instance) did not do \
             the work: read its last message first, since the error can come after it did part of it, send it the work again once",
            "If `cornercase send` or `cornercase mark` warns that a stage has not run since the last change, run \
             that stage first, or say in your final summary why not.",
        ],
    );
}

mod waking {
    use super::*;

    fn coder() -> Vec<(String, u64)> {
        vec![("coder".into(), 3)]
    }

    #[test]
    fn a_failed_role_did_not_do_the_work() {
        assert_eq!(
            failed(&coder()),
            "coder (pane 3) ended its turn on an error (such as an API error) before finishing the work you sent it, \
             so that work was not done. Read what it last wrote with `cornercase read --last-message --pane 3`, then \
             send it the work again; if it fails again, tell the user with `cornercase notify`."
        );
        let two = failed(&[("a".into(), 1), ("b".into(), 2)]);
        assert!(two.starts_with("a (pane 1) and b (pane 2) ended their turn on an error"), "{two}");
        assert!(two.contains("what they last wrote with `cornercase read --last-message --pane 1 --pane 2`"), "{two}");
    }

    #[test]
    fn the_wake_joins_what_applies() {
        assert!(woken(true, &[], &[]).starts_with("Your last turn ended on an error"));
        assert_eq!(woken(false, &[], &coder()), failed(&coder()));
        assert_eq!(woken(false, &coder(), &[]), finished(&coder()));
        let mixed = woken(true, &[("reviewer".into(), 4)], &coder());
        assert!(mixed.starts_with("reviewer (pane 4) finished the work"), "{mixed}");
        assert!(mixed.contains(". coder (pane 3) ended its turn"), "{mixed}");
        assert!(mixed.ends_with("go on with the plan from where you stopped."), "{mixed}");
        assert_eq!(woken(false, &[], &[]), "");
    }

    #[test]
    fn no_wake_holds_a_word_the_fake_agent_branches_on() {
        let texts = [
            woken(true, &[], &[]),
            woken(false, &[], &coder()),
            woken(false, &coder(), &[]),
            woken(true, &[("reviewer".into(), 4)], &coder()),
        ];
        for text in texts {
            for word in ["ask", "panel", "survey", "questions", "subagent", "background", "api-error", "/compact"] {
                assert!(!text.contains(word), "{word} in {text}");
            }
        }
    }
}

mod owed_stages {
    use super::*;
    use crate::flows::brief::owed;

    #[rstest::rstest]
    #[case::one(&[1], Some(0), "stage 2 (reviewer, security) has not run since the last change in stage 1: run it first, or say in your summary why not")]
    #[case::two(&[1, 2], Some(0), "stage 2 (reviewer, security) and stage 3 (tester) have not run since the last change in stage 1: run them first, or say in your summary why not")]
    #[case::three(&[0, 1, 2], None, "stage 1 (coder), stage 2 (reviewer, security) and stage 3 (tester) have not run since the last change: run them first, or say in your summary why not")]
    fn the_stages_owed_are_told_with_their_roles(
        #[case] stages: &[usize],
        #[case] changed: Option<usize>,
        #[case] expected: &str,
    ) {
        assert_eq!(
            owed(&builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0), stages, changed).as_deref(),
            Some(expected)
        );
    }

    #[test]
    fn nothing_owed_is_not_told() {
        assert_eq!(owed(&builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0), &[], Some(0)), None);
    }
}
