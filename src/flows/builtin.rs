use super::brief::WAIT_FIRST;
use super::{Back, End, Flow, ORCHESTRATOR, Role, Stage};
use crate::agents::Known;

const ORCHESTRATE: &str = "Keep every message to an agent self-contained: say what to do and what to answer with. \
     Pass findings and failures on word for word, with the agent that found them.";

const CODER: &str = "You are the coder in a team of coding agents working in this git worktree. Another agent, the \
     orchestrator, sends you the work. Implement it here, run the project's checks, and commit locally with clear \
     messages. Do not push and do not open pull requests. End each turn with a short summary of what you changed and \
     what you checked.";

const REVIEWER: &str = "You are a code reviewer in a team of coding agents working in this git worktree. Do not \
     edit files and do not commit. When the orchestrator asks, review the changes on this branch against its base \
     (`git log` and `git diff`). Answer with numbered findings, most severe first, each with the file and line, why it \
     is a problem and how to fix it, or with `No findings.` when there are none.";

const SECURITY: &str = "You are the security reviewer in a team of coding agents working in this git worktree. Do \
     not edit files and do not commit. When the orchestrator asks, review the changes on this branch against its base \
     for security problems: injection, unchecked input, secrets, unsafe file and process handling, permissions, \
     risky dependencies. Answer with numbered findings, most severe first, each with the file and line, the attack and \
     the fix, or with `No findings.` when there are none.";

const TESTER: &str = "You are the tester in a team of coding agents working in this git worktree. Do not edit \
     tracked files and do not commit; scratch files outside the repository are fine. When the orchestrator asks, \
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

fn orchestrator(agent: Known) -> Role {
    Role { name: ORCHESTRATOR.into(), agent: agent.name().into(), prompt: ORCHESTRATE.into(), ..Role::default() }
}

fn stage(roles: &[&str], back: Option<(usize, &str, u32)>) -> Stage {
    Stage {
        roles: roles.iter().map(|r| (*r).to_string()).collect(),
        back: back.map(|(to, when, at_most)| Back { to, when: when.to_string(), at_most }),
    }
}

pub(crate) fn flows(installed: &[Known], default: Option<Known>) -> Vec<Flow> {
    let agents = Agents::new(installed, default);
    let main = agents.main;
    let second = agents.other(&[main]);
    let third = agents.other(&[main, second]);
    vec![
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
        },
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
        },
        Flow {
            name: "Quick".into(),
            description: "A coder and one round of review, leaving the commits.".into(),
            orchestrator: orchestrator(main),
            roles: vec![role("coder", main, true, CODER), role("reviewer", second, false, REVIEWER)],
            stages: vec![stage(&["coder"], None), stage(&["reviewer"], Some((1, "findings", 1)))],
            end: End::Commits,
            ..Flow::default()
        },
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
        },
    ]
}

pub(crate) fn starter(installed: &[Known], default: Option<Known>) -> Flow {
    let agents = Agents::new(installed, default);
    let main = agents.main;
    Flow {
        name: "New flow".into(),
        orchestrator: orchestrator(main),
        roles: vec![role("coder", main, true, CODER), role("reviewer", agents.other(&[main]), false, REVIEWER)],
        stages: vec![stage(&["coder"], None), stage(&["reviewer"], Some((1, "findings", 3)))],
        end: End::PullRequest,
        ..Flow::default()
    }
}

pub(crate) fn member(name: &str, installed: &[Known], default: Option<Known>) -> Role {
    role(name, Agents::new(installed, default).main, false, MEMBER)
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
