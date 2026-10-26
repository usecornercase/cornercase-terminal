pub(super) const ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse";
pub(super) const LONG_ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse.

Without a command, cornercase opens its window, starting the server if none is running. The other
commands drive the running server from scripts, git hooks and the agents in its panes; they never
start a server, except `start-server`.";
pub(super) const AFTER_HELP: &str =
    "Ids come from `cornercase status` and last while the server runs. Inside a pane, the commands
act on that pane, its tab, workspace and project unless told otherwise; elsewhere on what the window
shows. Where a command takes --workspace ID, --worktree BRANCH names the workspace on that branch
instead, in that same project. Only `focus`, `go` and `--focus` change what the window shows, or with --view ID what that view shows (see `cornercase view`). The
commands exit with 1 on errors and timeouts, 2 on wrong usage, a branch no workspace is on
included, or one several are on (both list the workspaces to pick from), and 3 when a command that
asks several sources (usage, issue list, read --last-message with several panes) printed its whole
answer but some of them failed: each failed entry says why, in its error with --json.

Examples:
  cornercase status
  pane=$(cornercase new-tab --name tests -- cargo test)
  cornercase wait --pane \"$pane\" --until shell --timeout 900
  cornercase read --pane \"$pane\" --lines 40
  cornercase start claude --worktree fix-login --prompt 'Fix the login form' --wait";
pub(super) const STATUS_HELP: &str =
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
one before. It ends when the server stops or restarts; to go on, run it again, and its first line
is the state then, so nothing is missed. A reader that falls behind reads at most four older
lines, then the latest state.

With --app it prints, as JSON, what is outside the session: `todos` (as `todo list --json`), `queue`
(the entries of `queue list --json`), `away` (one row per chat app: `platform`, `ready`, `away`),
`update` (the release found and its notes, why there is none, whether a check or an install runs,
and how installing works here) and `keep_awake` (null while the setting is off), with the server's
`version` and `boot`. With --follow it keeps running and prints one such line first and again each
time one of them changes, each with a `revision` that grows by one per line, built at most twice a
second while someone follows; rules for ending and for slow readers are --follow's.

Examples:
  cornercase status
  cornercase status --json
  cornercase status --panes | awk -F'\\t' '$7 == \"waiting\" { print $1 }'
  cornercase status --panes --json | jq '.panes[] | select(.agent) | .pane'
  cornercase status --follow | jq -c '[.revision, (.projects | length)]'
  cornercase status --app --follow | jq -c '[.revision, (.todos | length)]'";
pub(super) const OPEN_HELP: &str = "Examples:
  cornercase open ~/src/shop
  cornercase open . --focus";
pub(super) const NEW_WORKSPACE_HELP: &str =
    "The workspace gets one tab. A worktree goes in the worktrees folder from settings, on a new
branch from HEAD when the branch does not exist yet; the command returns once git made it. The
project's setup (settings → project settings) then runs in the new tab, which `start --worktree`
waits for before it types the agent.

Examples:
  cornercase new-workspace experiments
  cornercase new-workspace fix/login --worktree --project 3";
pub(super) const COMMAND_HELP: &str =
    "The words of COMMAND are joined with spaces and typed at the prompt once the shell is ready,
as ssh does, so quote what the shell should read as one argument. The command returns once it
is typed.";
pub(super) const NEW_TAB_HELP: &str = "Examples:
  pane=$(cornercase new-tab --name server -- npm run dev)
  cornercase new-tab --workspace 5 -- 'cargo test 2>&1 | tee test.log'";
pub(super) const SPLIT_HELP: &str = "Examples:
  cornercase split -- htop
  cornercase split --pane 7 --down -- tail -f server.log";
pub(super) const START_HELP: &str =
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

Examples:
  cornercase start claude --issue '#482'
  cornercase start codex --issue ENG-42 --project 3
  cornercase start claude --worktree fix-login --prompt 'Fix the login form, then commit'
  cornercase start codex --prompt-file task.md --wait --timeout 1800
  cornercase start claude --mode plan --model opus --prompt 'Plan the login rework'";
pub(super) const SEND_HELP: &str =
    "The text goes in as one paste, bracketed when the program asked for it. A pane whose agent
waits for an answer to a question or a permission prompt is refused, since the text would answer
it; use `cornercase keys` for that. So is a Claude Code agent that shows a dialog, a panel or
its shell mode instead of its input box (`dialog` in `cornercase status --json`), unless you pass
--force. So is its feedback survey above the input box (`1: Bad  2: Fine  3: Good  0: Dismiss`,
`survey` in the JSON), where a digit sent alone would answer it: dismiss it with
`cornercase keys --pane N 0`, or pass --force. So is a pane whose agent is resuming its conversation
after a restart (`resuming` in the JSON), where the text would reach its shell or its start-up:
send again once it is back, or pass --force.

With --enter, the command returns once Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI or
GitHub Copilot CLI has recorded the prompt in its own history, and fails with `not confirmed: the prompt may not
have been submitted` when it records none: read the pane before sending it again. A working Codex,
Gemini, Grok Build, Cursor CLI or GitHub Copilot CLI holds a prompt until it takes it, so the command returns then.
Grok Build queues that prompt; pressing Enter again sends it now and cancels the turn. Gemini local
slash commands, ? help and ! shell input, and the / and ! commands of Grok Build and Cursor CLI, and the / and !
commands and a lone ? of GitHub Copilot CLI, return after Enter without confirming a prompt record. A compact command
(`/compact` of Claude Code and Codex) returns once the agent records the compaction, however long that takes
while the agent works on it; --timeout bounds it. In other programs it returns once Enter is pressed.

With --wait, the command fails if the agent does not start working within 10 seconds of the Enter,
and otherwise prints how the wait ended; with --until it waits for that state instead, as
`cornercase wait --until` does.

Examples:
  cornercase send --pane 12 --enter 'Now add tests for it'
  cornercase send --pane 12 --enter --wait --timeout 900 - < next-step.md
  cornercase send --pane 12 --enter --wait --until turn-over 'Run the tests'";
pub(super) const KEYS_HELP: &str = "Keys are encoded the way the program in the pane asked for.

Examples:
  cornercase keys --pane 12 ctrl+c
  cornercase keys --pane 12 down enter";
pub(super) const UNTIL_AFTER: &str = "Wait until the pane is in this state instead";
pub(super) const KEY_NAMES: &str = "enter, esc, tab, backspace, space, up, down, left, right, home, end, pageup,
pagedown, delete, insert, f1 to f12 or one character, each after any of ctrl+, alt+ and shift+";
pub(super) const READ_HELP: &str =
    "Lines the terminal wrapped come back joined, and empty lines at the end are left out.

With --last-message it prints the last message the agent in the pane wrote, as plain text, from
its own record (Claude Code's transcript, Codex's rollout, Gemini's session file, opencode's
database, Grok Build's session, Cursor CLI's chat store, GitHub Copilot CLI's session events) instead of the
screen, so it comes whole even once it scrolled off, without the input box or status lines. While the agent works
it is the newest one so far. --json adds when it was written and whether the agent's turn is over. It fails on a
pane without Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI or GitHub Copilot CLI, and before
the agent wrote anything.

With --last-message, repeat --pane or --tab to read several agents at once. Each message has a
header with its pane, agent, turn_over and written time; --json gives {\"panes\": [...]}. Panes come
first, then tabs, in the order given, and a repeated id is read once. Errors stay with their target,
and it exits 3 after reading them all if any failed; with no server it prints nothing and exits 1.
One target keeps the output above. Reading the screen takes only one target.

Claude Code's feedback survey (`How is Claude doing this session? (optional)`, then
`1: Bad  2: Fine  3: Good  0: Dismiss`) may show above its input box: it is Claude Code asking you,
not the agent's output. `cornercase keys --pane N 0` dismisses it.

Examples:
  cornercase read --pane 12
  cornercase read --pane 7 --lines 200 > build.log
  cornercase read --pane 12 --last-message
  cornercase read --tab 4 --last-message --json
  cornercase read --last-message --pane 12 --pane 15 --json";
pub(super) const WAIT_HELP: &str =
    "By default it waits until the agent stops working: idle, done, failed, pending (marked for review) or
waiting. A Claude Code agent whose turn is over while a background shell it started still runs
counts as working, since it wakes up when the shell ends; --until turn-over also ends there, and
prints shell. Subagents an agent started keep every wait waiting, --until turn-over included: the
agent goes on when they end. It then prints how it ended (idle, done, failed, pending, waiting, working, shell or quiet),
or the line that matched.
Waiting for an agent needs OSC 7501 reports, or a reader for Claude Code, Codex, Gemini CLI,
opencode, Grok Build, Cursor CLI or GitHub Copilot CLI in the pane. Without reports, Gemini status needs its dynamic window title,
and Cursor CLI's waiting needs /status-indicators. A timeout exits with 1 and does not prove that the
agent missed what you sent: read the pane before sending it again.

Several panes and tabs take --any or --all, and the condition applies to each. --any returns once
one of them meets it and prints one line, its id and how it ended; --all returns once every one has
met it, and prints one such line each, in the order given. The id is the one given, so a tab's line
starts with the tab's id. A pane that closes meanwhile ends as `closed` instead of failing the wait.

Examples:
  cornercase wait --pane 12 --timeout 600
  cornercase wait --pane 7 --until shell
  cornercase wait --pane 12 --until turn-over --timeout 600
  cornercase wait --pane 7 --text 'test result: (ok|FAILED)'
  cornercase wait --any --pane 12 --pane 15 --tab 9 --timeout 1800";
pub(super) const CLOSE_HELP: &str =
    "Its shells stop, as with its ×, but nothing asks first, not even for a project. A worktree's
workspace stays listed while its worktree exists, and its branch is kept.
--remove-worktree runs the project's teardown first; a teardown that fails is a warning on
stderr, and the worktree is removed anyway. --check only says what removing it would lose, as the
window's dialog does (changes that are not committed, a lock and whether its process still runs, the
worktrees of a flow's roles that go with it, the teardown), and stops nothing. --unlock does what
the dialog's unlock and remove does; changes still need --force.
--agent closes the agent in --pane as its close agent in the window does, without asking: its tab
and the tabs of the agents it started in the same workspace, at any depth. For a member of a flow
run, its whole team closes and the run ends. Agents it started elsewhere keep running. Nothing is
removed from disk, a worktree and its branch stay (close --workspace ID --remove-worktree removes
a worktree). With --check it only says what would stop and keep running.

Examples:
  cornercase close --tab 9
  cornercase close --workspace 5 --remove-worktree
  cornercase close --worktree fix/login --remove-worktree
  cornercase close --worktree fix/login --remove-worktree --check
  cornercase close --worktree fix/login --remove-worktree --unlock
  cornercase close --agent --pane 12 --check
  cornercase close --agent --pane 12";
pub(super) const RENAME_HELP: &str = "Examples:
  cornercase rename 'review #42'
  cornercase rename --workspace 5 ''";
pub(super) const FOCUS_HELP: &str =
    "Shows it in the window, or with --view ID in that view only (`cornercase view`), which then
follows it: the window and other views stay where they are.

--reveal TEXT (a pane only) also scrolls the pane to the text and selects it, as Enter on a result of
`cornercase search` does: TEXT is what the search looked for, and --line is the match's line from
`search --json` for a screen match. Leave --line out for an agent's prompt or reply, which takes the
newest place the pane shows the text. The scroll and the selection are the pane's, so the window and
every view show them. When the pane no longer holds the text it is still shown, a warning goes to
stderr and the exit status is 0.

--json prints the answer: `revealed` with --reveal, and with --view the view's new tab and its
`projection`, the number a view's lines and surface frames carry, which goes up each time what the
view shows changes.

Examples:
  cornercase focus --pane 12
  cornercase focus --pane 12 --reveal 'migration failed' --line 340
  cornercase focus --view 41 --tab 7";
pub(super) const GO_HELP: &str =
    "Does what a click on an agent's desktop notification does, or the go button of its toast: shows
the agent's pane (the one still needing you, when the notice covered several), or its workspace once
they are all closed, and prints the pane's id. With --view ID it moves that view (see
`cornercase view`) instead of the window. TOKEN is a notice's, from `cornercase events --notices`.
It fails for a token this server never gave or no longer keeps (the last 64; a restart forgets them),
and when the agent and its workspace are gone.

Examples:
  cornercase events --notices --json | jq -r 'select(.event == \"notice\") | .token'
  cornercase go cc-3f2a9c-4
  cornercase go cc-3f2a9c-4 --view 41";
pub(super) const NOTIFY_HELP: &str =
    "The desktop notification goes through the terminal of every open window, like the one for an
agent that needs you.

Examples:
  cornercase notify the release build is ready";
pub(super) const AWAY_HELP: &str =
    "It needs a Telegram or Discord bot paired with your phone (Settings › Remote), and turns away on
for each one paired. While you are away, an agent that waits for you or ends its turn sends its
question or its last message to that chat, and what you reply goes to the agent, until you turn it
off: here with --off, from the chat with /back, or from Remote in the window.

Examples:
  cornercase away
  cornercase away --off";
pub(super) const SEARCH_HELP: &str =
    "Looks in what the coding agents in the panes were told and answered (Claude Code, Codex, Gemini CLI,
opencode, Grok Build, Cursor CLI and GitHub Copilot CLI, the same records as `read --last-message`) and in the history of every
pane, shells included, ignoring case. Panes of projects being closed and workspaces being removed
are left out. It prints one line per match, newest first within a pane, its columns separated by
tabs under a header: the pane, the agent or program, where the text was found (prompt, reply or
screen), `project › workspace › tab`, the line on the pane's screen (screen matches only), and the
text. A missing value is -. No match prints the header alone and exits 0. With --json each match
also has its tab, workspace and project ids and, for an agent's message, when it was written.

Examples:
  cornercase search 'migration failed'
  pane=$(cornercase search --json checkout | jq -r '.matches[0].pane')
  cornercase focus --pane \"$pane\" --reveal checkout";
pub(super) const MARK_HELP: &str =
    "A pane marked as pending review shows a blue ● on its tab, and on its workspace, project and
group rows, until it is marked as reviewed or its agent starts working again on a new prompt or an
answer to its question. Looking at the tab does not take it off, unlike the ✓ of an agent that finished while you looked elsewhere.
Both marks are saved with the session, so they come back after a restart with the conversation.

Examples:
  cornercase mark
  cornercase mark --pane 12
  cornercase mark --pane 12 --reviewed";
pub(super) const TODO_HELP: &str = "Examples:
  cornercase todo add review the login fix
  cornercase todo list
  cornercase todo done 3
  cornercase todo undone 3
  cornercase todo edit 3 review the login fix again
  cornercase todo move 3 --before 5
  cornercase todo move 3 --last
  cornercase todo rm 3
  cornercase todo clear-done
  cornercase todo restore 7
  cornercase todo send 3 --pane 5

`rm` and `clear-done` print a number that `restore` takes to put back what they removed, where it was;
the server keeps the last 16 removals until it stops. An item moves among its kind: before an item of
the other kind it stops at the end of the ones to do, or at the first done one. `done` checks an item
off, or back on; `undone` only puts a done item back.";
pub(super) const TODO_SEND_HELP: &str = "Types the item's text into the agent and presses Enter, as
`cornercase send --enter` does, with the same target, refusals and confirmation that the agent took
the prompt. Once it did, the item is checked off. A refused or unconfirmed send leaves it pending,
prints the reason and exits with status 1. When cornercase cannot confirm the send at all (a local
`/` command, an agent it cannot watch), the text is still sent, the item stays to do, and the command
prints that and exits 0, with `unconfirmed: true` in --json. `checked` in --json says whether the
item was checked off; it is false when the item was edited or removed while it was sent. Only a
pending item can be sent.

Examples:
  cornercase todo send 3
  cornercase todo send 3 --pane 5 --json";
pub(super) const EVENTS_HELP: &str = "It keeps running and prints one line per event, as it happens:
  pane 5  claude  working → waiting   an agent changed state: working, shell (its turn is over but a
                                      background shell it started still runs), waiting, done, idle,
                                      or none once it left the pane
  pane 7  cargo  exited               the program in the foreground ended and the shell is back
  tab 3  opened in workspace 2        a project, workspace, tab or pane opened, or closed
With --notices, each notice about an agent is told too, as the window's toast and desktop notification
say it, with the same rules (a second's wait, once per change, never for a tab seen in the window or
in a view), and with --json its token, agent, status, place, message and panes:
  workspace 2  notice  claude needs you in shop › main  cc-3f2a9c-4
`cornercase go TOKEN` then does what a click on that notification does.
With --host, a program's own requests to the terminal are told too, for programs that draw the panes:
  pane 7  clipboard  11 bytes         it wrote to the clipboard (OSC 52); the text itself only with --json
  pane 7  bell  3 times               it rang the bell: one line a second per pane, with how many rang
With --json, each line is a JSON object with the event, the ids, the old and new state and the time.

It ends when the server stops, or once every pane given with --pane has closed. Programs are looked
at every half second while something follows events, so one that ends sooner may not show. Start it
before what you want to follow, or read `cornercase status` once it runs, so nothing falls between.
A reader that falls far behind loses events, and a line says how many.

Examples:
  cornercase events
  cornercase events --pane 12 --pane 14 --json
  cornercase events --host --json
  cornercase events --notices --json";
pub(super) const SURFACE_HELP: &str =
    "For programs that draw cornercase's terminals themselves, such as an editor extension or a
GUI. It keeps running and prints the tab as one JSON object per line: first as it is now, then again
each time something it shows changes, at most about 60 times a second. Each line is complete and
replaces the one before. It holds the panes laid out at the given size, each with its place (rect),
its terminal's own size (size), its cells (with their hyperlinks), the colours its program changed,
cursor, modes and scroll position, plus the split tree, the dividers, and boot, session and revision.

Each line names its stream with `surface`, which `cornercase resize` takes to lay a --tab surface out at
another size. With --tab the surface resizes no terminal: terminals keep the size of whoever sizes the tab (the
window, or a view), so size can differ from rect. With --view ID it streams the tab that view
shows, at the view's size, and follows the view to other tabs (each line carries view and
projection); it pauses while the view is inactive or has no size, and ends when the view closes.
The window keeps showing what it showed, also when the tab is not the one on screen. It ends when the tab closes or
the server stops (status 0); stopping it leaves the shells running. A reader that falls behind gets
fewer lines, each complete.

With --popup it streams the plugin popup open in the window (see `cornercase plugin run`) instead of a tab:
one pane, its program's terminal, at the popup's own size, with `popup` naming the plugin and the action,
`tab` 0 and no layout. It ends when the popup closes. `cornercase input --popup` types into it.

With --patches, each line after the first carries only what changed since the line before: `base` is the
revision it applies to, and each changed pane has its cursor and what changed of the rest (`lines` by
row number, `copy` for rows that moved). A complete line, without `base`, comes again whenever the layout,
a size, the panes or the view change, after the reader fell behind, and when `cornercase redraw --surface
ID` asks for one.

Examples:
  cornercase surface --tab 3 --cols 120 --rows 40
  cornercase surface --tab 3 --cols 120 --rows 40 | jq -c '[.revision, [.panes[].pane]]'
  cornercase surface --tab 3 --cols 120 --rows 40 --patches
  cornercase surface --view 41
  cornercase surface --popup";
pub(super) const INPUT_HELP: &str = "For programs that draw cornercase's terminals themselves (see `cornercase surface`). Each
EVENT is one JSON object, sent in order:
  {\"key\": {\"code\": \"c\", \"mods\": [\"ctrl\"]}}   code as for `keys`; mods ctrl, alt, shift, super; kind press, repeat, release
  {\"text\": \"héllo\"}                           typed text, without control characters
  {\"paste\": \"two\\nlines\"}                     bracketed when the program asked for it
  {\"mouse\": {\"kind\": \"down\", \"button\": \"left\", \"x\": 3, \"y\": 1}}
                                             kind down, up, drag, moved, scroll-up, scroll-down,
                                             scroll-left, scroll-right; x and y are cells of the
                                             pane's terminal, from 0
- reads one event per line from standard input.

Events are encoded the way the program in the pane asked for (its keyboard protocol, application
cursor keys, bracketed paste, mouse mode), whatever the window shows: a dialog or a field there
takes nothing. Keys, text and pastes bring the pane's view back to the bottom. Mouse events and key
releases that the program does not take are not sent, and said on stderr. When one event is wrong,
or the program is not reading its input, nothing is sent. Scripts and agents should use `send` and
`keys`, which check before typing into an agent. With --popup the events go to the plugin popup open
in the window, its x and y being cells of the popup's terminal; add --pane with the popup's id from its
surface lines (popup.pane) so that nothing is sent if that popup closed and another one opened.

Examples:
  cornercase input --pane 12 '{\"text\": \"ls\"}' '{\"key\": {\"code\": \"enter\"}}'
  cornercase input --pane 12 '{\"mouse\": {\"kind\": \"scroll-up\", \"x\": 10, \"y\": 5}}'
  cornercase input --popup --pane 40 '{\"key\": {\"code\": \"down\"}}' '{\"key\": {\"code\": \"enter\"}}'";
pub(super) const SCROLL_HELP: &str =
    "Moves the pane's view through its history, as the wheel does in the window, which shows the same
view. On its alternate screen, or when it reads the mouse, a program scrolls itself: send it keys or
the wheel with `cornercase input`.

Examples:
  cornercase scroll --pane 12 --up 10
  cornercase scroll --pane 12 --bottom";
pub(super) const SELECTION_HELP: &str =
    "Without --from and --to, prints what is selected in the pane. With them, selects from one cell to
another of its view, columns and rows from 0 as `cornercase surface` numbers them; the window shows
the same selection. --copy also copies it to the clipboard through the window, as selecting there
does, and prints what it copied, so a program without a window can put it in the clipboard itself.

Examples:
  cornercase selection --pane 12 --from 0,3 --to 79,5
  cornercase selection --pane 12 --copy
  cornercase selection --pane 12 --clear";
pub(super) const RESIZE_HELP: &str =
    "Lays a running `cornercase surface` out at another size, without starting it again: its next line
comes at that size. ID is the `surface` of its lines. The terminals keep the size the window gives
them.

Example:
  cornercase resize --surface 4 --cols 100 --rows 30";
pub(super) const REDRAW_HELP: &str =
    "Sends a running `cornercase surface` a complete line now, like its first one, even when nothing
changed. ID is the `surface` of its lines. A program that applies patches asks for one when it lost
track of the lines it applied.

Example:
  cornercase redraw --surface 4";
pub(super) const RATIO_HELP: &str =
    "RATIO is the share of the divider's first side, between 0 and 1. --path names the divider as a
surface line's `path` does, first or second at each level down; without it, the outermost one. The
terminals follow at once.

Examples:
  cornercase ratio --tab 3 0.3
  cornercase ratio --tab 3 --path second 0.5";
pub(super) const LINK_HELP: &str =
    "For programs that draw cornercase's terminals themselves (see `cornercase surface`). Prints the
file a click on that cell opens in the window: the path a program printed there, looked up from the
folder of the program in the foreground, then from the root of the pane's workspace, with its line or
lines (path:12, path:12-30). CELL is the column and row of the pane's view, from 0, as a surface frame
numbers them. Prints nothing when the cell holds no such path. --json also says whether the file is
inside the pane's workspace and which cells the path takes.

Examples:
  cornercase link --pane 12 --cell 10,3
  cornercase link --pane 12 --cell 10,3 --json";
pub(super) const THEME_HELP: &str =
    "Programs in the terminals ask for the colours they are drawn with (OSC 10, 11 and 4, and the light
or dark query): Claude Code, Neovim and delta pick their look from the answer. The server takes them
from the first window, so a program that drives cornercase without one gives them here, and changes
them when the system switches between light and dark.

FILE is JSON with the foreground, the background and the 16 ANSI colours as [r, g, b], and up to 256
palette entries (entries 16 and up are optional, null keeps the default):
  {\"foreground\": [192, 202, 245], \"background\": [26, 27, 38], \"palette\": [[21, 22, 30], ...]}
Use - to read it from standard input.

These colours win over a window's until you give others, --clear, or the server stops; a window that
attaches later keeps drawing with its own colours and leaves them alone. Programs that asked to hear
about colour scheme changes (mode 2031) are told when light and dark switch. Without FILE or --clear,
it says where the colours come from: default, window or client.

Examples:
  cornercase theme dark.json
  cornercase theme - < light.json
  cornercase theme --clear
  cornercase theme --json";
pub(super) const CONFIG_HELP: &str =
    "Every setting of the Settings dialog, with its value, default, choices and the note the dialog
shows, whether each account and bot is connected (and as whom), refused or not set, and each
project's setup and teardown. `--json` prints the whole answer, which a program can draw a form from.

`set` changes settings with the dialog's own checks and applies them at once, as the dialog does;
nothing is saved unless every value is accepted. Values are converted by the setting's type: true or
false for a switch, a whole number, a comma-separated list for issue_tabs, otherwise text.

A token is never an argument: `secret` reads it from standard input (hidden on a terminal), checks it
with its service as the dialog does, and says which account accepted it, never the token. `remove`
forgets a token, a bot, a paired chat or the paired browsers.

`export` writes the settings a colleague can use, as TOML, and says on standard error what it left
out: tokens, accounts, personal keys. `import` shows what a shared file would change and asks first;
reading standard input or without a terminal it needs --yes. --preview prints that preview and exits
without asking or importing: the same text on standard output, or, with --json, the preview a program
draws. --only limits either one to some sections. --preview and --yes cannot be combined.

The server notices edits to config.json within a second and applies them. What cornercase changes
meanwhile only rewrites the keys it touches, and it does not write over a file it cannot read.

Examples:
  cornercase config
  cornercase config --json
  cornercase config set sidebar tree
  cornercase config set fetch_minutes 10 counts false
  cornercase config set issue_tabs github,linear
  printf '%s' \"$LINEAR_API_KEY\" | cornercase config secret linear
  cornercase config remove linear
  cornercase config project ~/code/web-shop --setup 'npm ci'
  cornercase config export --to team.toml
  cornercase config import team.toml --only agents,worktrees
  cornercase config import team.toml --preview --json";

pub(super) const ISSUE_HELP: &str =
    "These read the trackers through the server's own connections, the ones the Issues dialog uses:
GitHub through gh, the others through the tokens in settings (connect one with
`cornercase config secret SOURCE`). Nothing is changed at a tracker.

sources says which trackers are connected for a project and as whom, what is missing (a token,
Jira's site and email, Plane's workspace, a repository) and why one fails, then where an issue
that isn't tied to a repository can be worked on. list prints one line per issue, separated by
tabs under a header; --closed, --assignee and --author are the dialog's filters: me, anyone, a
handle, or for Jira, Plane and Asana an id from `issue people`. A tracker that fails is named on
stderr, its cached list is still printed, and the command exits 3 after listing the others. read prints an issue's details, its
description in Markdown and its comments. people lists whom a tracker's filters can name.

To work on one: `cornercase start --issue KEY`, or `cornercase flow run NAME --issue KEY`.

Examples:
  cornercase issue sources
  cornercase issue list --source github --assignee me
  cornercase issue list --closed --json
  cornercase issue read '#482'
  cornercase issue read https://linear.app/acme/issue/ENG-42/fix-login
  cornercase issue people jira";

pub(super) const SKILL_HELP: &str = "Install them for Claude Code, Codex and other agents with
  npx skills add usecornercase/cornercase-terminal --skill cornercase -g
or put this text in an AGENTS.md or CLAUDE.md.";

pub(super) const REMOTE_HELP: &str =
    "cornercase runs `ssh DESTINATION cornercase proxy`, so your ssh config, keys, agent and
ProxyJump apply, and starts the server there if none is running. The shells, agents and files stay
on that machine; this one only shows them, and copies and notifications go through this terminal.
If the connection drops, the window stays and reconnects by itself; Esc or quit gives up. Both
machines need the same version of cornercase. Reconnecting never asks for a password, so use a
key or an agent.

Examples:
  cornercase remote devbox
  cornercase remote me@10.0.0.5 --command '~/bin/cornercase'";
pub(super) const LOGS_HELP: &str =
    "The server writes one line per event: the time in UTC, the level, where it happened and what,
then key=value details. It says which windows attached, which commands ran and how they were
answered, what started and stopped, how long background jobs took, what each agent was doing and
each step of starting one. It never holds what is typed, pasted or shown in a pane, nor tokens.

The log is server.log in the state folder ($XDG_STATE_HOME/cornercase, else ~/.local/state/cornercase),
or next to the socket when CORNERCASE_SOCKET is set. Past 8 MiB it moves to server.log.1 and starts
over. Start the server with CORNERCASE_LOG=debug for more: every event it handled, every git run,
slow frames.

Examples:
  cornercase logs -n 50
  cornercase logs --follow
  cornercase kill-server && CORNERCASE_LOG=debug cornercase";
pub(super) const START_SERVER_HELP: &str =
    "Scripts and programs that drive cornercase without a window run it first: it starts the server
if none runs (no terminal needed), restores the saved session once, as the first window would, and
answers when its terminals exist. Agents that resume their conversations are still typing then.
Until a window attaches, terminals take the size given here; the first window takes it over, and
gives the terminals its colours unless --theme gave them. Run again, it starts nothing and says the
server is already running, with the size in use and where it comes from.

Examples:
  cornercase start-server
  cornercase start-server --cols 200 --rows 60 --json
  cornercase start-server --theme dark.json";
pub(super) const KILL_SERVER_HELP: &str =
    "Run from a terminal inside cornercase, it would stop the server it runs in, and this terminal with
it, so it asks first; without a terminal it refuses unless --yes is given. To stop another server from
inside cornercase, say which with CORNERCASE_SOCKET=/absolute/path/server.sock.

Examples:
  cornercase kill-server
  CORNERCASE_SOCKET=/tmp/cc-dev/server.sock cornercase kill-server";
pub(super) const RESTART_HELP: &str =
    "It lists what runs in the terminals and asks first, since every program in them stops; agents
resume their conversations afterwards. Every attached window reopens on the new server.

With --when-idle, it waits until no agent is working before restarting: idle, done, pending review
and waiting for you count as stopped, and so does a Claude Code agent whose turn is over while a
shell it started in the background still runs, but not one whose subagents still run. The server restarts in the same step it sees that,
so no agent starts a turn in between, and every window shows that a restart is pending meanwhile,
with a button that cancels it. Programs that are not agents, such as a dev server, stop as with any restart. The pane
this runs in never holds the restart. --timeout gives up with status 1 and the server keeps running,
as it does when the command is interrupted.

Examples:
  cornercase restart
  cornercase restart --when-idle --yes --timeout 3600";
pub(super) const UPDATE_HELP: &str =
    "With --when-idle, it installs the release at once, then restarts the server on it once no agent
is working, as `cornercase restart --when-idle` does.

Examples:
  cornercase update
  cornercase update --when-idle --yes";
pub(super) const WHEN_IDLE: &str = "Wait until no agent is working, then restart";
pub(super) const RESTART_TIMEOUT: &str =
    "With --when-idle, give up after this many seconds and keep the server running";
pub(super) const PANE_REQUIRED: &str = "The pane to act on; `cornercase status --panes` lists them";
pub(super) const HERE_PANE: &str = "The pane [default: the one this runs in, else the shown one]";
pub(super) const WAIT_PANE: &str = "The pane; repeat it, or add --tab, to wait on several [default: the one this runs in, else \
    the shown one]";
pub(super) const WAIT_TAB: &str = "A tab: its agent's pane, else its active one; repeat it to wait on several";
pub(super) const HERE_WORKSPACE: &str = "The workspace [default: the one this runs in, else the shown one]";
pub(super) const SEEN_DIGEST: &str = "Act only on the version with this digest, from `changes list --json` or `changes file --json` (the Uncommitted one for stage, unstage and discard): a file that changed since is left alone [default: the version there now]";
pub(super) const BY_BRANCH: &str =
    "The workspace on this branch, looked up in the project this runs in, else the shown one";

pub(super) const CONTRACT_HELP: &str =
    "Programs that drive cornercase without this command line (an editor extension, a dashboard) ask
the server for it before anything else. `contract` is the version of the requests and answers: it
goes up only when one changes in a way an older client would misread, while new commands, fields
and events are added without it. `boot` names this run of the server and changes on every restart,
since ids last only one run. Each command is listed with whether it is available, and each stream
with its version. A server without this command is older than the contract.

Examples:
  cornercase contract
  cornercase contract --json";

pub(super) const MOVE_HELP: &str =
    "Puts a group, a project, a workspace or a tab before another of its kind, or at the end of its
list with --last, as dragging its row does. A project moved before another joins that project's group;
--into puts it after the projects of a group, --ungroup after the ones that have none. A workspace
stays in its project and a tab in its workspace. Programs keep running, and the window keeps showing
what it showed. Say the place every time, so a forgotten flag never sends a row to the end.

Examples:
  cornercase move --tab 9 --before 7
  cornercase move --worktree fix/login --last
  cornercase move --project 5 --into 12
  cornercase move --project 5 --ungroup
  cornercase move --group 12 --before 3";
pub(super) const MOVE_PANE_HELP: &str =
    "Swaps two panes of a tab, or moves one to the left, right, top or bottom of another, as dragging
a pane with the right button does. Both keep running. It is refused when the tab would have no room
for every pane at the size it has. The moved pane becomes the tab's active one. With --json it prints
the tab's new layout.

Examples:
  cornercase move-pane --pane 4 --target 7 --place swap
  cornercase move-pane --pane 4 --target 7 --place below --json";
pub(super) const FOLD_HELP: &str =
    "Sets it, whatever it was: a group shows or hides its projects, a project its workspaces and a
workspace its tabs; an agent's pane (--pane) hides the agents it started in the agents section.
Its shells keep running.

Examples:
  cornercase collapse --group 12
  cornercase collapse --pane 12
  cornercase expand --worktree fix/login";
pub(super) const GROUP_HELP: &str =
    "`new` takes the next icon and colour, as + New › new group does, and prints the group's id. `style`
without --icon and --colour prints the group's style and the icons and colours to choose from.
`delete` keeps the projects, outside any group, unless --close-projects closes them too, which
deletes the group once their shells have stopped.

Examples:
  cornercase group new work
  cornercase group style 12 --icon '★' --colour 99
  cornercase group style 12 --json
  cornercase group delete 12 --close-projects";

pub(super) const CHANGES_HELP: &str =
    "`list` prints the changed files of a workspace (the one you are in, or the one --workspace or --worktree names), or of \
every project with --every, and with --follow keeps printing them as JSON lines each time they change. `file` prints one \
file's diff, `gap` the unchanged lines between one of its hunks and the one before (hunks count from 0, so the first has \
no gap), `base` the branches the Commits and All modes can compare with (naming one sets it, --default goes back to \
following the default branch), and `viewed` marks a file as viewed or clears the mark. `discard` throws a file's uncommitted changes away (a file git does not track is deleted), asks first on a terminal \
and needs --yes in a script; it cannot be undone. `stage` puts a file's changes in git's index, as `git add` does, and \
`unstage` takes them out of it. `viewed`, `stage`, `unstage` and `discard` act on the version `changes file` shows; with \
--digest, on the version whose digest `changes list --json` or `changes file --json` printed, the one you saw, so a file \
that changed since is left alone (`viewed` does not mark it, the others fail and say so). \
The modes are uncommitted (the default), \
commits and all. The base and the viewed marks are shared with the window's changes panel; the mode of a stream is its own.

Examples:
  cornercase changes list
  cornercase changes list --every --follow
  cornercase changes list --worktree fix/login --mode commits --json
  cornercase changes file src/main.rs --json
  cornercase changes gap src/main.rs --hunk 1
  cornercase changes base origin/main
  cornercase changes viewed src/main.rs
  cornercase changes stage src/main.rs
  cornercase changes unstage src/main.rs
  cornercase changes discard src/main.rs --yes
  cornercase changes discard src/main.rs --digest 550a7a517a389403 --yes";

mod flow;

pub(in crate::cli) use flow::{
    FLOW_DEFAULT_HELP, FLOW_DELETE_HELP, FLOW_EXPORT_HELP, FLOW_FILES_HELP, FLOW_HELP, FLOW_HIDE_HELP,
    FLOW_IMPORT_HELP, FLOW_MESSAGES_HELP, FLOW_ORDER_HELP, FLOW_RESTORE_HELP, FLOW_RUN_HELP, FLOW_RUNS_HELP,
    FLOW_SAVE_HELP,
};

#[cfg(test)]
mod tests;

pub(super) const PLUGIN_HELP: &str =
    "A plugin is a folder with a cornercase-plugin.toml that declares commands: actions in the \
row menus, hooks on events, services the server keeps running and processes each local window keeps running. Its \
commands talk back through these same commands.

Install one from GitHub, any https:// git address or a git folder, pinned to a commit; a repository can hold \
several plugins, each in its own folder (owner/repo/folder on GitHub, address//folder elsewhere): you see where it comes \
from, what it asks for and every command it runs before anything runs, and it is installed turned off. Link a \
folder of yours instead while you write one.

`--preview` fetches a plugin, or an update of one, and prints that same preview, then exits without asking or \
installing, also with no terminal; with `--json` (and an ID for `update`) it is one JSON object, for a program that \
shows it and asks itself. `--commit` then installs, or updates to, exactly the commit the preview printed: it fails \
and installs nothing once the source gives another commit, and unlike SOURCE@COMMIT, which pins the plugin there, \
later updates still follow its branch or tag.

A plugin runs as you and can do anything you can, including typing into every terminal. What it asks for is shown \
before you turn it on, not enforced. A running server picks up changes within a second.

`actions` lists what plugins add to the menus, for a kind of row (--context) or all of them, and `run` runs one on \
a project, workspace, tab or pane as its menu entry does: in the workspace's folder, with the ids of what you \
name, stopped after 5 minutes, its output in the plugin's log. Both ask the running server; `run` waits for the \
action and exits with 1 when it fails. An action on a plugin's button runs with no target, on what the window \
shows. A popup action waits until its popup closes, and needs a window attached; `popup-close` closes it. An action \
listed before the plugins changed is refused (pass the --revision `actions --json` printed, or none for the current \
one).

`status` prints the plugins as Settings › Plugins shows them: on, off or cannot run, the label and the note, and \
what each service is doing (running, restarting or still starting), and why `plugins.json` cannot be read. It asks \
the running server. `list` still reads the files and works with no server.

Examples:
  cornercase plugin install acme/notes
  cornercase plugin install acme/notes@v1.2.0
  cornercase plugin install acme/notes --preview --json
  cornercase plugin update acme.notes --preview
  cornercase plugin install acme/tools/plugins/notes
  cornercase plugin install https://git.example.com/acme/tools.git//plugins/notes
  cornercase plugin link ~/code/my-plugin
  cornercase plugin enable my-plugin
  cornercase plugin list
  cornercase plugin status
  cornercase plugin status --json
  cornercase plugin actions --context pane
  cornercase plugin run acme.notes open --pane 5
  cornercase plugin run acme.notes browse
  cornercase plugin popup-close
  cornercase plugin logs my-plugin --follow
  cornercase plugin disable my-plugin
  cornercase plugin update
  cornercase plugin remove notes";

pub(super) const VIEW_ARG: &str = "Act in this view (see `cornercase view`): its tab is the default, and --focus moves the view instead of the window";
pub(super) const VIEW_HELP: &str =
    "For programs that draw cornercase themselves, such as an editor extension or a GUI, so that each
picks its own tab and size without touching the window or the others. It keeps running and prints
the session as one JSON line each time it changes, like `status --follow`, with a `view` object:
id, projection, active, cols, rows, shown (the project, workspace, tab and pane it shows) and
controls (whether it sizes its tab's terminals now). The view lives while this command runs:
stopping it closes the view and gives its tabs back. The first line is the state now; the view's id
is in it. Keep it running in the background (`cornercase view --cols 120 --rows 40 > view.jsonl &`),
read the id from the first line, and pass it as --view ID to `focus`, `new-tab`, `split`, `start`,
`send`, `keys`, `read`, `wait`, `status`, `rename`, `mark`, `go` or `surface`: the view's tab is their
default and --focus moves it.

While the view is active, has a size and is not hidden, the tab it shows counts as seen like the
window's: an agent there gets no done mark, toast or desktop notification. --hidden starts it hidden
(`view-set --hidden` and `--visible` change it), for a window that is minimised or behind others:
agents in its tab then notify again. While a view is open, the window's tab counts as seen only while
a window is attached.

A tab's terminals take the size of the window or the view that claimed it last, without the
window's sidebar and panels: a view claims the tab it shows when it opens, moves there or turns
active, and using the window takes back the tab it shows. Only that one resizes them. Without
--cols and --rows the view only follows the session and sizes nothing. Shared with the window and
other views: the processes, the split layout, the focused pane of a tab, scrolling and selection.

Examples:
  cornercase view --cols 120 --rows 40
  cornercase view --cols 120 --rows 40 --tab 7";
pub(super) const VIEW_SET_HELP: &str =
    "Changes a view that `cornercase view` holds open. --cols and --rows resize it, --inactive makes it
size nothing and stream no terminals (its tabs go to whoever claimed them before, else the window),
--active turns it back on and claims its tab, and --claim takes its tab back from the window or
another view. --hidden says its window is hidden or minimised, so agents in its tab notify again, and
--visible says it is back. It prints the view as it is now.

Examples:
  cornercase view-set --view 41 --cols 100 --rows 30
  cornercase view-set --view 41 --claim
  cornercase view-set --view 41 --hidden";
pub(super) const TOUR_HELP: &str =
    "Shows the guided tour in the window: a card that points at a part of the screen and moves on
when you do what it asks. Without an action it starts the core tour where it stopped. A new
install shows it once by itself; `tour` in Settings › UI turns that off. `list` prints every tour
and its steps as data (ids, titles, texts, what each step points at and what completes it) with
your progress, so another UI can draw the same tour; --json prints it as {\"state\", \"tours\"}.
`start --no-card` moves the tour without drawing the window's card.

Examples:
  cornercase tour
  cornercase tour start todo
  cornercase tour list --json
  cornercase tour reset";

pub(super) const PORTS_HELP: &str =
    "Lists the TCP ports in LISTEN state held by the programs of a workspace: the processes its panes
started (and their children), and, in a workspace with a worktree of its own, any of your processes
working in that worktree. Each line gives the URL to open (http://localhost:PORT when it listens on
every address or on 127.0.0.1), the addresses, the program and its pid, and how it belongs to the
workspace: pane N, agent claude in pane N (the agent itself or a program it started), or cwd. A
plain workspace (no worktree of its own) shares its folder with the others, so only its panes count.
--all lists every port your processes listen on, those of no workspace too. Ports published by
Docker are held by Docker, not by the workspace that started the containers, so they show only
with --all. With --json it prints {\"ports\": [...]}: port, addresses, url, pid, program, args,
project, workspace, pane, via (pane, agent or cwd) and agent, null when they do not apply.

The text shows the program and its arguments cut to 40 characters; --json gives them whole.
`cornercase ports stop PORT` stops one (see its --help).

Examples:
  cornercase ports
  cornercase ports --worktree feat/login --json
  cornercase ports --all
  cornercase ports stop 5173";

pub(crate) const ISSUE_LIST_HELP: &str =
    "A tracker that fails does not stop the others: it is named on stderr with its reason, its cached
list is still printed (with --json, its entry in sources has an error), and the command exits 3
after listing the others.";

pub(crate) const PORTS_STOP_HELP: &str =
    "Stops the program that listens on PORT, if it belongs to a workspace (`cornercase ports --all`
lists the others, which it refuses). It checks first that the program still holds the port, sends
it SIGTERM, waits up to 3 s for the port to be released, then sends SIGKILL. Only that process is
signalled, never its process group, so the shell or the agent that started it stays. When several
programs listen on the port, --pid says which. It prints stopped, killed (it ignored SIGTERM) or
still running, and exits with 1 when it is still running. With --json it prints {\"port\",
\"pid\", \"program\", \"outcome\"}, the outcome being stopped, killed or still-running.

Examples:
  cornercase ports stop 5173
  cornercase ports stop 3000 --pid 48211 --json";

pub(super) const USAGE_HELP: &str =
    "Asks Claude Code and Codex for your plan limits as the Limits tab of the Usage dialog does, and
prints each limit with how much of it is used and when it resets, then extra usage or credits. It
runs `claude` and `codex` on the server's machine (the ones agent_commands names): the installed
ones, or both when neither is, each saying why it could not run. That sends no prompt and costs no
tokens, and each waits at most ten seconds. A probe already running, from the dialog or another
`cornercase usage`, is waited for rather than started again. With --cached it asks nothing and
prints the last answers with their age. With --json it prints {\"agents\": [...]}: each agent's
plan, its limits (label, kind, scope, percent, severity, resets_at in UTC), extra, notice, error,
and when it was read (read_at, and age in seconds). It exits with 3 when an agent could not be
asked, after printing them all, that one with its error.

Examples:
  cornercase usage
  cornercase usage --cached --json";

pub(super) const ACCOUNT_HELP: &str =
    "An account is a folder of its own under cornercase's state folder in which an agent keeps its
login: CLAUDE_CONFIG_DIR for Claude Code, CODEX_HOME for Codex, GEMINI_CLI_HOME for Gemini CLI.
Each agent has a default account, which new workspaces, scripts run outside a pane and the queue
start with: `account use`, or Settings › Accounts in the window, changes it. A new tab or a split
takes the accounts of the pane it comes from, and panes already open keep theirs until
`account restart`, or the account button under the pane, restarts them on another, each once its
turn is over, in the same conversation. `account rename` gives an account (or the agent's
own login, with --system) the name the window shows; commands always name an account by its
folder. `start`, `new-tab`, `flow run` and `queue` take --account AGENT=ACCOUNT to start once
with another. A new account links the settings it shares from the agent's own folder (settings,
instructions, skills); `account list` names the ones that stopped being links. Log in once per
account, with --login or by running the agent in a tab of that account. Removing an account
deletes its folder, never the system's keychain, and an agent that used it goes back to its own
login.

Examples:
  cornercase account add claude work --login
  cornercase account use claude work
  cornercase account restart claude --pane 4
  cornercase account rename claude work Work
  cornercase account list
  cornercase start claude --account claude=home --prompt \"fix the tests\"
  cornercase account remove claude work";

pub(super) const PROFILE_HELP: &str =
    "A profile, for scripts, names which account each agent uses. While a profile is active, it is the
default: new workspaces and agents use its accounts; panes already open keep the ones they started
with. Choosing an account with `account use`, or in Settings › Accounts, leaves the active profile;
`profile use --system` goes back to each agent's own login. `start`, `new-tab` and `flow run` take
--profile to use another one once, without changing the default.

Examples:
  cornercase profile new Work claude=work codex=work
  cornercase profile set Work --provider codex --account personal
  cornercase profile use Work
  cornercase start claude --profile Home --prompt \"fix the tests\"";

pub(super) const AGENTS_HELP: &str =
    "`close` closes every tab of a workspace (the one you are in, or the one --workspace or --worktree names) that runs \
a coding agent, whole, and keeps what each agent needs to start again: its conversation, mode and account. Shells and \
the workspace stay. It refuses when closing would cut an agent's turn, when an agent cannot resume its conversation \
(opencode, custom agents, an agent with no conversation yet starts a new one) or when other panes share those tabs, \
unless you pass --force; --check says what would close and closes nothing. A flow's team closes with its orchestrator \
and the run is kept whole. `reopen` brings the tabs back after the others, each agent in its own conversation (even \
with resume_agents off), and a flow's orchestrator last with its team's new pane ids. `forget` drops what was closed \
for good, asks first on a terminal and needs --yes in a script, and ends a kept flow run. Closed agents survive a \
restart of the server and show as `closed_agents` in `status --json`; removing the workspace forgets them.

Examples:
  cornercase agents close
  cornercase agents close --worktree fix/login --check
  cornercase agents close --workspace 7 --force --json
  cornercase agents reopen --workspace 7
  cornercase agents forget --workspace 7 --yes
";
