use super::brief::WAIT_FIRST;
use super::{Back, End, Flow, ORCHESTRATOR, Role, Stage, Worktree};
use crate::agents::Known;

const ORCHESTRATE: &str = "Keep every message to an agent self-contained: say what to do and what to answer with. \
     Pass findings and failures on as the paths of the files that hold them, with the agent that found them, rather than their text.";

const CODER: &str = "You are the coder in a team of coding agents working in this git worktree. Another agent, the \
     orchestrator, sends you the work. Implement it here, run the project's checks, and commit locally with clear \
     messages. Do not push and do not open pull requests. End each turn with a short summary of what you changed and \
     what you checked.";

const COMPETITOR: &str = "You are a competing coder in a team of coding agents. You work in a git worktree of your \
     own, on a branch of your own, which the other coders cannot touch. The orchestrator sends every competitor the \
     same task and the same rubric. Implement it your way, run the project's checks, and commit on your branch: the \
     others read your work from its commits, so what is not committed does not count. Do not push and do not open \
     pull requests. End each turn with a short summary of the approach you took and what you checked.";

const JUDGE: &str = "You are the judge in a team of coding agents working in this git worktree. Do not edit files \
     and do not commit. The competing coders each work on a branch of their own, which the orchestrator names. When \
     the orchestrator asks, read every candidate with `git log`, `git diff base...branch` and `git show \
     branch:path`, and build and test each one in its own worktree folder. Run the checks or the benchmark the \
     orchestrator names. Score every candidate against the rubric with evidence, say which one to use as the base \
     and which parts of the others are worth taking, file by file. Answer with the scores, the recommendation and \
     the grafts.";

const INTEGRATOR: &str = "You integrate the winning candidate in a team of coding agents working in this git \
     worktree. The orchestrator names the branch to take and the parts of the others to graft. Merge that branch \
     here with `git merge`, apply the grafts by hand or with `git cherry-pick`, run the project's checks, and commit \
     with clear messages. Do not push and do not open pull requests. End each turn with a short summary of what you \
     merged and what you checked.";

const ARENA: &str = "Run it as a competition. Before any coder starts, write the rubric: what a good result is and \
     how it is measured (tests, a benchmark, size, how easy it is to extend), and send the same message with the \
     task and the rubric to every competing coder. Choose by the judge's evidence, not by how long an answer is, and \
     pick as the base the one a maintainer can extend. Take the good parts of the others by asking the integrator to \
     graft them by hand. If no candidate is good enough, converge or reframe the task once and ask the coders again. \
     Leave the losing worktrees as they are.";

const REVIEWER: &str = "You are a code reviewer in a team of coding agents working in this git worktree. Do not \
     edit the worktree's files and do not commit. When the orchestrator asks, review the changes on this branch against its base \
     (`git log` and `git diff`). Answer with numbered findings, most severe first, each with the file and line, why it \
     is a problem and how to fix it, or with `No findings.` when there are none.";

const SECURITY: &str = "You are the security reviewer in a team of coding agents working in this git worktree. Do \
     not edit the worktree's files and do not commit. When the orchestrator asks, review the changes on this branch against its base \
     for security problems: injection, unchecked input, secrets, unsafe file and process handling, permissions, \
     risky dependencies. Answer with numbered findings, most severe first, each with the file and line, the attack and \
     the fix, or with `No findings.` when there are none.";

const TESTER: &str = "You are the tester in a team of coding agents working in this git worktree. Do not edit \
     tracked files and do not commit; scratch files, logs and scripts go in the run's folder. When the orchestrator asks, \
     build the project, run its tests, and try the change for real the way a user would. Answer with `PASS` and what \
     you tried, or with numbered failures, each with the steps to reproduce it.";

const REPRODUCER: &str = "You reproduce bugs in a team of coding agents working in this git worktree. When the \
     orchestrator sends a bug, write the smallest test that fails because of it, run it to see it fail, and commit \
     it. Do not fix the bug. Answer with the test's name, how to run it and how it fails.";

const MEMBER: &str = "You are part of a team of coding agents working in this git worktree. Another agent, the \
     orchestrator, sends you the work and says what to answer with.";

struct Agents {
    installed: Vec<Known>,
    main: Known,
}

impl Agents {
    fn new(installed: &[Known], default: Option<Known>) -> Self {
        let main = default.filter(|kind| installed.contains(kind)).or_else(|| installed.first().copied());
        Self { installed: installed.to_vec(), main: main.or(default).unwrap_or(Known::Claude) }
    }

    fn other(&self, avoid: &[Known]) -> Known {
        self.installed.iter().copied().find(|kind| !avoid.contains(kind)).unwrap_or(self.main)
    }

    fn free(&self, wanted: Known, taken: &[Known]) -> Known {
        if !wanted.status_by_folder() || !taken.contains(&wanted) {
            return wanted;
        }
        self.installed.iter().copied().find(|kind| !kind.status_by_folder()).unwrap_or(wanted)
    }

    fn spread(&self, mut flow: Flow) -> Flow {
        let mut order: Vec<usize> = (0..flow.roles.len()).filter(|&i| !flow.roles[i].own_worktree()).collect();
        order.sort_by_key(|&i| !flow.roles[i].edits);
        let mut taken: Vec<Known> = Vec::new();
        let members = order.into_iter().map(Some).chain([None]);
        for member in members {
            let role = match member {
                Some(i) => &mut flow.roles[i],
                None => &mut flow.orchestrator,
            };
            let Some(wanted) = Known::of(&role.agent) else { continue };
            let agent = self.free(wanted, &taken);
            agent.name().clone_into(&mut role.agent);
            taken.push(agent);
        }
        flow
    }
}

fn role(name: &str, agent: Known, edits: bool, prompt: &str) -> Role {
    Role {
        name: name.to_string(),
        agent: agent.name().to_string(),
        edits,
        prompt: format!("{prompt} {WAIT_FIRST}"),
        ..Role::default()
    }
}

fn own(mut role: Role) -> Role {
    role.worktree = Worktree::Own;
    role
}

fn orchestrator(agent: Known) -> Role {
    Role { name: ORCHESTRATOR.into(), agent: agent.name().into(), prompt: ORCHESTRATE.into(), ..Role::default() }
}

fn competitors(agents: &Agents) -> Vec<Role> {
    let mut kinds = vec![agents.main];
    for _ in 0..2 {
        let next = agents.other(&kinds);
        if kinds.contains(&next) {
            break;
        }
        kinds.push(next);
    }
    if kinds.len() == 1 {
        kinds.push(agents.main);
    }
    kinds
        .into_iter()
        .zip(['A', 'B', 'C'])
        .map(|(kind, letter)| own(role(&format!("coder {letter}"), kind, true, COMPETITOR)))
        .collect()
}

fn arena(agents: &Agents, second: Known) -> Flow {
    let main = agents.main;
    let coders = competitors(agents);
    let names: Vec<&str> = coders.iter().map(|role| role.name.as_str()).collect();
    let first = stage(&names, None);
    let mut orchestrator = orchestrator(main);
    orchestrator.prompt = format!("{ORCHESTRATE} {ARENA}");
    let mut roles = coders;
    roles.push(role("judge", main, false, JUDGE));
    roles.push(role("integrator", main, true, INTEGRATOR));
    roles.push(role("reviewer", second, false, REVIEWER));
    Flow {
        name: "Arena".into(),
        description: "Competing coders, each in a worktree of its own, a judge, then one result: for measurable work \
                      or an unclear shape, not everyday features."
            .into(),
        orchestrator,
        roles,
        stages: vec![
            first,
            stage(&["judge"], None),
            stage(&["integrator"], None),
            stage(&["reviewer"], Some((3, "findings", 2))),
        ],
        end: End::PullRequest,
        ..Flow::default()
    }
}

fn stage(roles: &[&str], back: Option<(usize, &str, u32)>) -> Stage {
    Stage {
        roles: roles.iter().map(|r| (*r).to_string()).collect(),
        back: back.map(|(to, when, at_most)| Back { to, when: when.to_string(), at_most }),
    }
}

pub(crate) fn flows(installed: &[Known], default: Option<Known>) -> Vec<Flow> {
    let agents = Agents::new(installed, default);
    planned(&agents).into_iter().map(|flow| agents.spread(flow)).collect()
}

fn planned(agents: &Agents) -> Vec<Flow> {
    let main = agents.main;
    let second = agents.other(&[main]);
    let third = agents.other(&[main, second]);
    vec![
        full_issue(main, second, third),
        cross_review(main, second, third),
        quick(main, second),
        bug_test_first(main, second),
        arena(agents, second),
    ]
}

fn full_issue(main: Known, second: Known, third: Known) -> Flow {
    Flow {
        name: "Full issue".into(),
        description: "A coder, a reviewer and a security reviewer, then a tester, then a pull request.".into(),
        orchestrator: orchestrator(main),
        roles: vec![
            role("coder", main, true, CODER),
            role("reviewer", second, false, REVIEWER),
            role("security", third, false, SECURITY),
            role("tester", main, false, TESTER),
        ],
        stages: vec![
            stage(&["coder"], None),
            stage(&["reviewer", "security"], Some((1, "findings", 3))),
            stage(&["tester"], Some((1, "failures", 2))),
        ],
        end: End::PullRequest,
        ..Flow::default()
    }
}

fn cross_review(main: Known, second: Known, third: Known) -> Flow {
    Flow {
        name: "Cross review".into(),
        description: "A coder and two reviewers of different agents, until both agree.".into(),
        orchestrator: orchestrator(main),
        roles: vec![
            role("coder", main, true, CODER),
            role("reviewer", second, false, REVIEWER),
            role("second reviewer", third, false, REVIEWER),
        ],
        stages: vec![stage(&["coder"], None), stage(&["reviewer", "second reviewer"], Some((1, "findings", 3)))],
        end: End::PullRequest,
        ..Flow::default()
    }
}

fn quick(main: Known, second: Known) -> Flow {
    Flow {
        name: "Quick".into(),
        description: "A coder and one round of review, leaving the commits.".into(),
        orchestrator: orchestrator(main),
        roles: vec![role("coder", main, true, CODER), role("reviewer", second, false, REVIEWER)],
        stages: vec![stage(&["coder"], None), stage(&["reviewer"], Some((1, "findings", 1)))],
        end: End::Commits,
        ..Flow::default()
    }
}

fn bug_test_first(main: Known, second: Known) -> Flow {
    Flow {
        name: "Bug, test first".into(),
        description: "A failing test first, then the fix, then a tester, then a pull request.".into(),
        orchestrator: orchestrator(main),
        roles: vec![
            role("reproducer", second, true, REPRODUCER),
            role("coder", main, true, CODER),
            role("tester", main, false, TESTER),
        ],
        stages: vec![
            stage(&["reproducer"], None),
            stage(&["coder"], None),
            stage(&["tester"], Some((2, "failures", 2))),
        ],
        end: End::PullRequest,
        ..Flow::default()
    }
}

pub(crate) fn starter(installed: &[Known], default: Option<Known>) -> Flow {
    let agents = Agents::new(installed, default);
    let main = agents.main;
    agents.spread(Flow {
        name: "New flow".into(),
        orchestrator: orchestrator(main),
        roles: vec![role("coder", main, true, CODER), role("reviewer", agents.other(&[main]), false, REVIEWER)],
        stages: vec![stage(&["coder"], None), stage(&["reviewer"], Some((1, "findings", 3)))],
        end: End::PullRequest,
        ..Flow::default()
    })
}

pub(crate) fn member(name: &str, installed: &[Known], default: Option<Known>, taken: &[Known]) -> Role {
    let agents = Agents::new(installed, default);
    role(name, agents.free(agents.main, taken), false, MEMBER)
}

pub(crate) fn installed(config: &crate::config::Config) -> Vec<Known> {
    Known::ALL
        .into_iter()
        .filter(|kind| crate::agents::installed(&crate::agents::command(config, kind.name())))
        .collect()
}

pub(crate) fn for_config(config: &crate::config::Config) -> Vec<Flow> {
    flows(&installed(config), Known::of(&config.agent))
}
