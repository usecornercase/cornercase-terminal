pub(in crate::cli) const FLOW_HELP: &str =
    "A flow is a team of coding agents and the plan they follow: one tab per role in a new
worktree, every agent started without permission prompts, and an orchestrator agent that sends the
work from role to role with these commands. The built-in flows come with cornercase; yours live in
flows/ next to config.json, and a project's in .cornercase/flows/ in its repository.

Examples:
  cornercase flow list
  cornercase flow run Quick --prompt 'Fix the login form'
  cornercase flow run 'Full issue' --issue ENG-42 --project 3
  cornercase flow run 'Full issue' --prompt-file task.md --worktree fix-login
  cornercase flow export Quick > quick.toml
  cornercase flow import quick.toml";
pub(in crate::cli) const FLOW_RUN_HELP: &str =
    "It makes the worktree (or uses a worktree already on that branch, if it runs no flow), opens a
tab per role named after it, the orchestrator first, and starts every agent with its role prompt,
answering their trust prompts. The orchestrator starts last, with a brief: the task, the team and
its pane ids, the stages, and how to drive the agents. Nothing is shown in the window unless you
pass --focus.

With --issue KEY instead of --prompt, the task is the issue's, from the same prompt template and branch the
Issues dialog's start a flow uses; an issue that isn't tied to a repository needs --project when several
projects are repositories. Otherwise, without --worktree, the branch comes from the task's first words, with a short suffix when that
says nothing, is a default branch or is already taken. A chosen branch that is main, master, the
project's default or the one its own folder is on is refused: a flow works on a branch of its own.

A role set to work in a worktree of its own (worktree = own) gets one on the branch <branch>--<role>, made from the
run's branch, with the project's setup; the other roles work in the run's worktree.

It prints the workspace's id, then one line per role: its pane's id and the role's name, separated
by a tab. The run's folder for files, where every role writes its answers, is printed on stderr
(run_dir with --json) and is in $CORNERCASE_RUN_DIR in the team's panes. A name is looked up in yours, then the built-in flows, then the project's: yours replaces
a built-in flow of its name, and a project's flow never stands in for another of the same name; the
command says so on stderr when it happens. A built-in flow deleted in Settings › Flows is refused,
and keeps its name: a project's flow named like it does not run either.

Examples:
  cornercase flow run Quick --prompt 'Fix the login form'
  cornercase flow run 'Bug, test first' --prompt-file bug.md --worktree fix-crash --json";
pub(in crate::cli) const FLOW_EXPORT_HELP: &str =
    "It prints the flow as TOML, the file a flow is kept in, so a teammate can read its prompts, variables
and arguments before importing it. A name is looked up as `flow run` does. Variables a flow sets are
named on stderr: check no secret goes with them.

Examples:
  cornercase flow export Quick > quick.toml
  cornercase flow export 'Full issue' --json";
pub(in crate::cli) const FLOW_IMPORT_HELP: &str =
    "It reads a flow at most 256 KiB long, lists its roles, and saves it in yours (or the project's, with
--to project). The source is a file, - for standard input, an https:// address, or OWNER/REPO, a GitHub
repository with a flow.toml (or cornercase-flow.toml) at its root; usecornercase.dev/flows lists them.
Every agent of a flow runs without permission prompts, so a flow from an address or a repository,
which nobody reviewed, always asks first, and so does one that sets
variables or arguments; without a terminal it refuses unless you pass --yes.

A name that is already taken is refused: pass --replace to overwrite that flow, or --name to import
this one under another name. A built-in flow's name is yours to take, with --replace, and it then
stands in its place. A project's flow never takes the place of one of yours or a built-in.

Examples:
  cornercase flow import quick.toml
  cornercase flow import octocat/security-audit
  cornercase flow import https://example.com/flows/mine.toml
  cornercase flow import quick.toml --to project --name 'Quick mine'
  ssh host cornercase flow export Quick | cornercase flow import - --yes";
