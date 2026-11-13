pub(in crate::cli) const FLOW_HELP: &str =
    "A flow is a team of coding agents and the plan they follow: one tab per role in a new
worktree, every agent started without permission prompts, and an orchestrator agent that sends the
work from role to role with these commands. The built-in flows come with cornercase; yours live in
flows/ next to config.json, and a project's in .cornercase/flows/ in its repository.

Examples:
  cornercase flow list
  cornercase flow run Quick --prompt 'Fix the login form'
  cornercase flow run 'Full issue' --issue '#482' --fresh
  cornercase flow run 'Full issue' --issue ENG-42 --project 3
  cornercase flow run 'Full issue' --issue '#482' --prompt-file task.md
  cornercase flow run 'Full issue' --prompt-file task.md --worktree fix-login
  cornercase flow export Quick > quick.toml
  cornercase flow import quick.toml
  cornercase flow save draft.json --check
  cornercase flow runs
  cornercase flow messages --worktree fix-login --follow
  cornercase flow hide Quick
  cornercase flow default Quick
  cornercase flow order Quick 'Full issue'
  cornercase flow delete 'Quick mine'";
pub(in crate::cli) const FLOW_RUN_HELP: &str =
    "It makes the worktree (or uses a worktree already on that branch, if it runs no flow), opens a
tab per role named after it, the orchestrator first, and starts every agent with its role prompt,
answering their trust prompts. The orchestrator starts last, with a brief: the task, the team and
its pane ids, the stages (when the work goes back to an earlier stage, that stage and every stage
after it run again before the work goes on), and how to drive the agents. Nothing is shown in the window unless you
pass --focus, which shows the orchestrator's tab and opens the Flow panel; with --view ID it shows
that tab in the view instead, and the window and its Flow panel stay as they are.

With --issue KEY, the task, the branch and the workspace's name are the issue's: the same prompt template
and branch the Issues dialog's start a flow uses. --prompt or --prompt-file give a task of your own instead,
as editing that dialog's prompt does, and --worktree and --workspace-name name the branch and the
workspace. An issue that isn't tied to a repository needs --project when several
projects are repositories. Without --issue or --worktree, the branch comes from the task's first words, with a short suffix when that
says nothing, is a default branch or is already taken. A chosen branch that is main, master, the
project's default or the one its own folder is on is refused: a flow works on a branch of its own.

A role set to work in a worktree of its own (worktree = own) gets one on the branch <branch>--<role>, made from the
run's branch, with the project's setup; the other roles work in the run's worktree.

The run's branch, when it does not exist yet, starts from the project's HEAD, or, with --fresh or
settings → worktrees → new branches, from the project's default branch fetched from the remote first
(origin, else the first one), as a routine's run does; a fetch that fails stops the run before any tab
opens, and --no-fresh asks for HEAD whatever the setting says. Role branches start from the run's branch
either way. --fresh needs a server that knows the `fresh-branches` feature.

It prints the workspace's id, then one line per role: its pane's id and the role's name, separated
by a tab. The run's folder for files, where the task is saved as task.md before any agent starts (byte for
byte the run's task in `flow runs --json`) and where every role writes its answers, is printed on stderr
(run_dir with --json) and is in $CORNERCASE_RUN_DIR in the team's panes. A name is looked up in yours, then the built-in flows, then the project's: yours replaces
a built-in flow of its name, and a project's flow never stands in for another of the same name; the
command says so on stderr when it happens. A built-in flow deleted in Settings › Flows is refused,
and keeps its name: a project's flow named like it does not run either.

With --close-when-merged the run's agents close, as `agents close` does, once the run's pull request has merged
and nothing in the workspace has worked, waited, started or been typed into for 3 minutes. Nothing is removed: the workspace's ▶ or
`agents reopen` brings each agent back in its own conversation (opencode and custom agents start over), and a reopen
keeps them open. A flow that leaves the commits has no pull request, so it is refused. It needs a server that knows the
`flow-close-when-merged` feature; `flow close-when-merged` turns it on or off for a run that is already going.

With --free-builds too, once the agents closed it frees the build folders of the run's worktree and of its roles'
own worktrees, as `worktrees free-builds` does without --force: only git-ignored folders on the project's list
(node_modules, target...); a worktree where a program still runs, or whose build folder holds a git checkout,
keeps them, and the toast and the `flow-merged` message say what was freed or why not. The worktrees, their branches
and the run's folder stay, so reopening the agents rebuilds what they need. --free-builds needs --close-when-merged
and a server that knows the `flow-free-builds` feature.

With --machine NAME the flow runs on that machine of `cornercase machines`, by its server: the flow,
its agents and accounts are the machine's, --project and the ids printed are that machine's, as
`status --json` lists them under machines, and so is the run's folder. --focus shows the run in this
window once the machine lists it, and takes no --view there. It needs a server that knows the
`machine-start` feature. --boot does not go with --machine, whose ids are that machine's: send the
command there with --remote and its boot.

Examples:
  cornercase flow run Quick --prompt 'Fix the login form'
  cornercase flow run 'Bug, test first' --prompt-file bug.md --worktree fix-crash --json
  cornercase flow run 'Full issue' --issue '#482' --prompt-file task.md
  cornercase flow run Quick --issue '#482' --project 2 --machine vps
  cornercase flow run 'Full issue' --issue '#482' --close-when-merged
  cornercase flow run 'Full issue' --issue '#482' --close-when-merged --free-builds";
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

--check shows the roles, risks and missing agents and exits without asking or saving, even without
a terminal or when reading -. With --json it prints name, origin, path (null), replaced, roles,
check (the risks, when any) and note (missing agents, when any). A bad flow or a name conflict still
fails. --check and --yes cannot be combined.

A name that is already taken is refused: pass --replace to overwrite that flow, or --name to import
this one under another name. A built-in flow's name is yours to take, with --replace, and it then
stands in its place. A project's flow never takes the place of one of yours or a built-in.

Examples:
  cornercase flow import quick.toml
  cornercase flow import octocat/security-audit
  cornercase flow import quick.toml --check --json
  cornercase flow import https://example.com/flows/mine.toml
  cornercase flow import quick.toml --to project --name 'Quick mine'
  ssh host cornercase flow export Quick | cornercase flow import - --yes";
pub(in crate::cli) const FLOW_DELETE_HELP: &str =
    "It deletes the flow's file, from yours or from the project's, and nothing else: a run already going keeps
the plan it was started with. It never touches a built-in flow, which has no file: `flow hide` deletes one,
and a flow of yours that edits a built-in is deleted by `flow restore` (the built-in comes back) or
`flow hide` (the built-in stays deleted). Pass --in when yours and the project's have the same name, or
--path with the file `flow list --json` gives for a flow that cannot be read.

Examples:
  cornercase flow delete 'Quick mine'
  cornercase flow delete Review --in project
  cornercase flow delete --path ~/.config/cornercase/flows/broken.toml";
pub(in crate::cli) const FLOW_HIDE_HELP: &str =
    "A built-in flow lives in the binary, so deleting it hides its name: your choice, kept across updates,
which `flow restore` undoes. If you edited the flow, your version is deleted too. It is what Settings › Flows
does with the delete button on a built-in flow. Hiding one that is already hidden changes nothing.

Examples:
  cornercase flow hide Quick";
pub(in crate::cli) const FLOW_RESTORE_HELP: &str =
    "It brings a deleted built-in flow back, and when you edited it, deletes your version so the built-in
is the one that runs again. A flow that is shown and not edited has nothing to restore.

Examples:
  cornercase flow restore Quick";
pub(in crate::cli) const FLOW_DEFAULT_HELP: &str =
    "The start dialog (an issue's start a flow, or start a flow… in + New) opens with the default flow selected.
When the default is missing, hidden or has problems, it falls back to the first built-in flow that can
start, so a default never blocks a start. Deleting or hiding the default flow clears it; renaming it in
Settings › Flows moves it to the new name. It is the same as set default in Settings › Flows, and choosing
the flow that already is the default changes nothing. --clear chooses no default. `flow run NAME` still
needs the name: the default only picks what the start dialog selects.

Examples:
  cornercase flow default Quick
  cornercase flow default --clear";
pub(in crate::cli) const FLOW_ORDER_HELP: &str =
    "It puts the flows in the order you name them, for Settings › Flows, the start dialog and `flow list`, the
same as dragging them in Settings › Flows. The flows you leave out go after the ones you name, by name, as a new
built-in flow or an imported one does. A name that no flow has is refused, and so is one named twice. Names
kept from other projects' flows keep their place. --clear gives name order back. Naming the order the flows
already are in changes nothing.

Examples:
  cornercase flow order Quick 'Full issue'
  cornercase flow order --clear";
pub(in crate::cli) const FLOW_RUNS_HELP: &str =
    "It prints one block per running flow: its workspace, the stage the work is in and the rounds of its loops,
the role that holds the work, the last step along the plan and the last one outside it, each role's pane and
status (closed once its pane is gone), and whether the orchestrator marked its own pane for review, which is
how a run ends. Stages count from 1 here; --json counts them from 0, as the diagram's boxes do.

--follow prints the same runs as one JSON line each time something changes, the first line included, until the
server stops; each line has a revision that only goes up while the server runs. The answer of a run that
has no message yet or no step has them null.

After the work goes back to an earlier stage (a loop's round, or a step outside the plan), a run lists the
stages it has not been through since: `not run since the last change: stage 3, stage 4`, or, once the
orchestrator marked its pane, `ended with stage 3 and 4 not run on the last change` (not_run with --json,
counted from 0, left out when there is none). A first pass that skips a stage shows only as outside the plan,
and a restart forgets what was not run.

A run that closes its agents once its pull request merges says so (close_when_merged with --json), and one that frees its builds then too (free_builds).

A run that has stopped moving (its orchestrator's turn is over, no agent of the team works, it has not
ended and no wake is being sent) says `stopped moving 50 min ago: …` (stopped with --json, the UTC time it
stopped, left out while the run moves).

Examples:
  cornercase flow runs
  cornercase flow runs --follow";
pub(in crate::cli) const FLOW_CLOSE_WHEN_MERGED_HELP: &str =
    "It turns on, or with --off turns off, closing a run's agents once its pull request has merged and nothing in
its workspace has worked, waited, started or been typed into for 3 minutes, as `flow run --close-when-merged` does from the start.
The agents are kept, as `agents close` keeps them: the workspace's ▶ or `agents reopen` brings them back. A flow
that leaves the commits has no pull request, so it cannot be turned on; --off is always accepted. Without --workspace
or --worktree it is the workspace this runs in, else the shown one. It sets the choice, never toggles it, so
saying it twice changes nothing the second time. --free-builds also frees the build folders of the run's worktrees
once its agents close, as `flow run --free-builds` does; without it they are kept, and --off turns both off
(--free-builds with --off is refused).

Examples:
  cornercase flow close-when-merged --worktree fix-login
  cornercase flow close-when-merged --worktree fix-login --free-builds
  cornercase flow close-when-merged --worktree fix-login --off";
pub(in crate::cli) const FLOW_MESSAGES_HELP: &str =
    "It prints what the orchestrator and the roles of the run sent each other, as the Flow panel's arrows show it:
the orchestrator's sends, and the keys it pressed, the screens it read and the waits it made, with the text
that was sent. Only what this server saw is kept (the last 256, in memory, never saved), so a run that was
restored starts with none. The text is cut at 32 KiB; `bytes` says its real size.

--follow goes on printing each new message until the run ends. With --json each message is one line with
its run, workspace, seq (counted from 1 for the run), time, from, to, kind (send, keys, read or wait), text,
bytes and edge, the step along the plan it moved the work through, or null.

Examples:
  cornercase flow messages --workspace 4
  cornercase flow messages --worktree fix-login --follow --json";
pub(in crate::cli) const FLOW_FILES_HELP: &str =
    "It lists the files in the run's folder, where the task is (task.md) and every role writes its
answers: name, size, when it was changed and its path, tab-separated under a header, newest first, at most
200. After the run ended it lists the last folder the workspace's runs made, for as long as the workspace lives.

Examples:
  cornercase flow files
  cornercase flow files --worktree fix-login --json";

pub(in crate::cli) const FLOW_SAVE_HELP: &str =
    "It saves a flow written as JSON: the `flow` of an entry of `flow list --json` (`jq '.flows[0].flow'`), read from a
file or from standard input with -, checked as Settings › Flows does. It is for apps with an editor of their own;
people edit flows in Settings › Flows or in their files. Keys left out take their defaults, and a key a flow does
not have is refused with its place, so a typo never drops a setting.

A flow with problems of its own (a role in no stage, a loop that goes forward, an unknown agent...) is refused. One
whose name another flow runs under instead is saved, and the clash is printed, as the list shows it in red. --check
says whether the flow can be saved, prints the problems, and what a project's flow runs unattended, and saves
nothing; with --json, own_problems counts the problems of its own, which come first.

Without --path it saves a new flow, into yours or, with --to project, the project's: saving it again needs the
--path it printed, or it makes a second flow of that name. --path names the file of the flow being edited, as
`flow list --json` gives it: the flow stays in its place unless --to moves it, and a new name renames the file.
Saving a built-in flow's name into yours makes your version of it, as edit does in Settings › Flows.

Examples:
  cornercase flow list --json | jq '.flows[0].flow' > draft.json
  cornercase flow save draft.json --check
  cornercase flow save draft.json --path ~/.config/cornercase/flows/quick-mine.toml
  cornercase flow save - --to project --json < draft.json";
