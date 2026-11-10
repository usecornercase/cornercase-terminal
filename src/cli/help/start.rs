pub(in crate::cli) const NEW_WORKSPACE_HELP: &str =
    "The workspace gets one tab. A worktree goes in the worktrees folder from settings, on a new
branch from HEAD when the branch does not exist yet; the command returns once git made it. The
project's setup (settings → project settings) then runs in the new tab, which `start --worktree`
waits for before it types the agent.

With --machine NAME the workspace is made on that machine of `cornercase machines`, by its server:
--project is that machine's id, as `status --json` lists it under machines, and so is the id printed.
--focus shows it in this window once the machine lists it. It needs a server that knows the
`machine-start` feature.

Examples:
  cornercase new-workspace experiments
  cornercase new-workspace fix/login --worktree --project 3
  cornercase new-workspace fix/login --worktree --project 2 --machine vps --focus";
pub(in crate::cli) const START_HELP: &str =
    "The agent starts with its command and arguments from settings → agents. --mode replaces the mode
chosen there by another of the agent's modes, or drops it with --mode default, --model adds the agent's own model flag and --effort its
effort (Claude Code, Codex, Grok Build and GitHub Copilot CLI take one; an older server refuses all three rather than
start the agent without them). If it asks whether
you trust the folder, the question waits for your answer unless settings → agents accept it for
you. Once it is ready, the prompt is pasted and
submitted, and the command prints the pane's id; it fails if the agent never shows up. With
--wait, a second line says how the wait ended: idle, done (it finished out of sight) or waiting
(it needs an answer); with --until it waits for that state instead, as `cornercase wait --until` does.

--issue KEY (a key such as #482, sc-48 or ENG-42, or the issue's URL) starts the agent on that issue as
the Issues dialog's start does: GitHub and GitLab issues in a worktree on the branch named after the issue, of
the project's own repository; the prompt comes from settings' prompt template and is always submitted.
Shortcut, Linear, Jira, Plane and Asana issues aren't tied to a repository: when several places are open the
command lists them and asks for --project (a new worktree) or --workspace (a new tab). A workspace already
on the branch gets a new tab. It needs a server that knows the `issues` feature.

With --machine NAME the agent starts on that machine of `cornercase machines`, by its server, as if
the command ran there: --project, --workspace and the pane's id printed are that machine's ids, as
`status --json` lists them under machines, and the agent, its accounts, profiles and issue trackers
are the machine's own. --wait, --until and --timeout are refused: follow the agent there, with
`cornercase --remote`. --focus shows its tab in this window once the machine lists it. It needs a
server that knows the `machine-start` feature.

With --placed the command answers as soon as the tab is open, after its worktree is made or its
issue read, with the same ids, and the launch goes on, here or on a machine, as the window's dialogs
do: a trust question then waits in the tab, not in this command, and the prompt follows its answer.
It is for a program that shows the new tab itself, and it can no longer say that the agent never
showed up. --wait, --until and --timeout are refused, since nothing is waited for. It needs a server
that knows the `machine-start` feature, which came with it.

`cornercase wait` and `send` only work once that launch is over, and the answer does not mean it
is. Until the agent runs, wait says the pane runs no agent; until its prompt is typed, wait can end
on idle; and send goes to whatever reads the pane then, the shell, or the trust question, which it
would answer. `status --json` has no field for the launch, so read the pane there (under machines
for one on a machine): `agent` is null until the agent runs and reports, and `at_prompt` true while
its shell still waits for it; `agent` with the status idle only says it runs, since it may still be
asking its question and its prompt may not be typed; and the status working says it took its prompt,
so the launch is over. Without a prompt, or with an agent that reports no status, only its
screen says so (`cornercase read`). To send or wait right after the command, leave --placed out: its
answer then comes when the launch is over.

Examples:
  cornercase start claude --issue '#482'
  cornercase start codex --issue ENG-42 --project 3
  cornercase start claude --worktree fix-login --prompt 'Fix the login form, then commit'
  cornercase start codex --prompt-file task.md --wait --timeout 1800
  cornercase start claude --mode plan --model opus --prompt 'Plan the login rework'
  cornercase start claude --issue '#482' --project 2 --machine vps --focus
  cornercase start claude --issue '#482' --project 2 --machine vps --placed --json";
