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
chosen there by another of the agent's modes, --model adds the agent's own model flag and --effort its
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

Examples:
  cornercase start claude --issue '#482'
  cornercase start codex --issue ENG-42 --project 3
  cornercase start claude --worktree fix-login --prompt 'Fix the login form, then commit'
  cornercase start codex --prompt-file task.md --wait --timeout 1800
  cornercase start claude --mode plan --model opus --prompt 'Plan the login rework'
  cornercase start claude --issue '#482' --project 2 --machine vps --focus";
