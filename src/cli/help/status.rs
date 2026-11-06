pub(in crate::cli) const STATUS_HELP: &str =
    "Each pane shows its program and folder, and for a coding agent what it is doing (working,
waiting for an answer, done out of sight, failed, idle), its model and how full its context is. `(you)`
marks the pane running the command, `(shown)` what the window shows. `working (background shell)`
is a Claude Code agent whose turn is over while a shell it started in the background still runs:
it wakes up when that shell ends (`background_shell` in the JSON, where the status stays working).
`working (2 subagents)` is an agent whose own turn is over while subagents it started still run
(`subagents` in the JSON, the number): it goes on when they report back, so it stays working.
`(dialog open)` is a Claude Code agent showing a dialog, a panel or its shell mode instead of its
input box, which `cornercase send` refuses (`dialog` in the JSON).
`(survey open)` is Claude Code's feedback survey above its input box, which `cornercase send`
refuses too (`survey` in the JSON): `cornercase keys --pane N 0` dismisses it.
`(resuming)` is a pane whose agent is coming back in its conversation after a restart: until it
is back, it takes no input and `cornercase send` refuses it (`resuming` in the JSON).
`(role coder)` marks a flow member and the role it plays (`role` in the JSON).
`(collapsed)` is an agent whose children are hidden in the agents section (`collapsed` in the JSON).
`Opus 5.5 · compacted` is an agent whose conversation was just compacted: its context is empty
until its next reply and comes back then (`compacted` in the JSON, there only while true).

With --panes it prints one line per pane instead, for scripts, in the sidebar's order: a header
line naming the columns, then one row per pane, its columns separated by tabs. The columns are
pane, tab, workspace, project (their ids), program, agent, status, background_shell, dialog,
at_prompt, model, context, caller (the pane running the command), shown (the pane the window
shows), active (its tab's active pane), tab_name, workspace_name, branch, worktree, project_name,
path, survey (Claude Code's feedback survey shows), resuming (the agent is coming back in its
conversation), parent (the pane that started it), subagents (how many it still runs) and role (the
role it plays in a flow, from its run) and compacted (true while the context is empty because the
conversation was just compacted, the last column); new ones only ever go at the end. Values are as in the
JSON (true or false, context as a number, full paths), with - where there is none and a space for a tab or line break in a name.
With --json, it prints {\"panes\": [...]}, one object per pane with the columns as keys and null
where there is no value.

With --follow it keeps running and prints the whole state as one JSON line: first as it is now,
then again shortly after each time something in it changes, always JSON, with
--json or without. Each line is what --json prints plus `revision`, which grows with every line,
with `caller` always null;
`boot` names the server's run, and revisions only compare within one boot. Each line replaces the
one before. It ends when the server stops or restarts, or once what reads it is gone; to go on, run
it again, and its first line
is the state then, so nothing is missed. A reader that falls behind reads at most four older
lines, then the latest state.

With --app it prints, as JSON, what is outside the session: `todos` (as `todo list --json`), `queue`
(the entries of `queue list --json`), `away` (one row per chat app: `platform`, `ready`, `away`),
`update` (the release found and its notes, why there is none, whether a check or an install runs,
and how installing works here) and `keep_awake` (null while the setting is off), with the server's
`version` and `boot`. `accounts_revision` moves each time `account list` would name other accounts,
and is left out until it first does, so a client lists them again only when it moves. With --follow
it keeps running and prints one such line first and again each time one of them changes, each with
a `revision` that grows by one per line, built at most twice a second while someone follows; rules
for ending and for slow readers are --follow's.

Examples:
  cornercase status
  cornercase status --json
  cornercase status --panes | awk -F'\\t' '$7 == \"waiting\" { print $1 }'
  cornercase status --panes --json | jq '.panes[] | select(.agent) | .pane'
  cornercase status --follow | jq -c '[.revision, (.projects | length)]'
  cornercase status --app --follow | jq -c '[.revision, (.todos | length)]'";
