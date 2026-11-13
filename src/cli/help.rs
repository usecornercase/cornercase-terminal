pub(super) const ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse";
pub(super) const LONG_ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse.

Without a command, cornercase opens its window, starting the server if none is running. The other
commands drive the running server from scripts, git hooks and the agents in its panes; they never
start a server, except `start-server`. With --remote DESTINATION they drive the server of another
machine, through ssh.";
pub(super) const AFTER_HELP: &str =
    "Ids come from `cornercase status` and last while the server runs. Inside a pane, the commands
act on that pane, its tab, workspace and project unless told otherwise; elsewhere on what the window
shows. Where a command takes --workspace ID, --worktree BRANCH names the workspace on that branch
instead, in that same project. Only `focus`, `go` and `--focus` change what the window shows, or with --view ID what that view shows (see `cornercase view`). The
commands exit with 1 on errors and timeouts, 2 on wrong usage, a branch no workspace is on
included, or one several are on (both list the workspaces to pick from), and 3 when a command that
asks several sources (usage, issue list, files list, read --last-message with several panes) printed its whole
answer but some of them failed, input --follow could not send some lines, or a line of requests --follow failed: each failed entry says
why, in its error with --json. A command given --boot (close, rename, mark, move-pane, collapse,
expand, account restart, context, send, input, paste-file) exits with 4 when it names another run
of the server, which then did nothing, and every command that sends a request exits with 5 when the cornercase --remote
reaches is older than this one (update, restart, kill-server, logs and the plugin commands that
work on its files run there without checking), printing with --json both versions (there is null when it is too old to say) and the command that
updates it as one JSON line. `update` exits with 6 when this cornercase cannot replace itself, after printing the command
to run instead.

--remote DESTINATION (or CORNERCASE_REMOTE=DESTINATION, which the flag overrides) sends a command
to the server of another machine through ssh, found as `cornercase remote` finds it, so arguments and
standard input arrive as they are. The other machine may run this version or a newer one (an older one
is refused with exit status 5, and `--remote DESTINATION update` updates it), ssh never asks for a
password, only `start-server` starts a server there, and a command that follows a stream ends it
there when it ends here. Paths are the other machine's: absolute, or starting with ~/ for its home folder; `files` also takes
them relative to the workspace; `paste-file` reads its FILE here. `update`, `restart`,
`kill-server`, `logs` and the plugin commands that work on its files (`plugin link`, `install`,
`enable`, `list`, `logs`…) run there, as that machine's own command through ssh, and exit with its
status; `plugin windows` refuses it.

Examples:
  cornercase status
  cornercase --remote devbox status --json
  pane=$(cornercase new-tab --name tests -- cargo test)
  cornercase wait --pane \"$pane\" --until shell --timeout 900
  cornercase read --pane \"$pane\" --lines 40
  cornercase start claude --worktree fix-login --prompt 'Fix the login form' --wait";
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
pub(super) const SEND_HELP: &str =
    "The text goes in as one paste, bracketed when the program asked for it. A pane whose agent
waits for an answer to a question or a permission prompt is refused, since the text would answer
it; use `cornercase keys` for that. So is a Claude Code agent that shows a dialog, a panel or
its shell mode instead of its input box (`dialog` in `cornercase status --json`), unless you pass
--force. So is its feedback survey above the input box (`1: Bad  2: Fine  3: Good  0: Dismiss`,
`survey` in the JSON), where a digit sent alone would answer it: dismiss it with
`cornercase keys --pane N 0`, or pass --force. So is a pane whose agent is resuming its conversation
after a restart (`resuming` in the JSON), where the text would reach its shell or its start-up:
send again once it is back, or pass --force. While a person is typing in the pane the text waits for them
and goes right after their prompt (`held_sends` in the JSON); `cornercase held` acts on it from elsewhere.
If they choose send now while they have a draft, the text is pasted after it without its Enter, and the
command fails saying it is in their input.

With --enter, the command returns once Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI,
Vibe CLI or GitHub Copilot CLI has recorded the prompt in its own history, and fails with `not confirmed: the prompt may not
have been submitted` when it records none: read the pane before sending it again. A working Codex,
Gemini, Grok Build, Cursor CLI, Vibe CLI or GitHub Copilot CLI holds a prompt until it takes it, so the command returns
then.
Grok Build queues that prompt; pressing Enter again sends it now and cancels the turn. Gemini local
slash commands, ? help and ! shell input, and the / and ! commands of Grok Build and Cursor CLI, and the / and !
commands and a lone ? of GitHub Copilot CLI, and the / (but not /skill), ! and & commands of Vibe CLI, return after Enter without
confirming a prompt record. A compact command
(`/compact` of Claude Code and Codex) returns once the agent records the compaction, however long that takes
while the agent works on it; --timeout bounds it. In other programs it returns once Enter is pressed.

--boot BOOT says which run of the server the pane or tab id is from: `boot` in `status --json`, or in
a machine's entry there. Ids last one run, so a server that restarted since refuses, nothing is
sent, and the command prints why and exits with 4.

With --wait, the command fails if the agent does not start working within 10 seconds of the Enter,
and otherwise prints how the wait ended; with --until it waits for that state instead, as
`cornercase wait --until` does.

Examples:
  cornercase send --pane 12 --enter 'Now add tests for it'
  cornercase send --pane 12 --enter --wait --timeout 900 - < next-step.md
  cornercase send --pane 12 --enter --wait --until turn-over 'Run the tests'
  cornercase --remote vps send --pane 9 --enter --boot 4242-1700000000 'Now add tests for it'";
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
database, Grok Build's session, Cursor CLI's chat store, GitHub Copilot CLI's session events, Vibe CLI's unified session store) instead of the
screen, so it comes whole even once it scrolled off, without the input box or status lines. While the agent works
it is the newest one so far. --json adds when it was written and whether the agent's turn is over. It fails on a
pane without Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI, Vibe CLI or GitHub Copilot CLI, and before
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
and Cursor CLI's waiting needs /status-indicators. Vibe CLI's status comes only from its reports. A timeout exits with 1 and does not prove that the
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
--boot BOOT says which run of the server the id is from: `boot` in `status --json`, or in a machine's
entry there. Ids last one run, so a server that restarted since refuses, nothing closes, and the
command prints why and exits with 4.

Examples:
  cornercase close --tab 9
  cornercase close --workspace 5 --remove-worktree
  cornercase close --worktree fix/login --remove-worktree
  cornercase close --worktree fix/login --remove-worktree --check
  cornercase close --worktree fix/login --remove-worktree --unlock
  cornercase close --agent --pane 12 --check
  cornercase close --agent --pane 12
  cornercase --remote vps close --workspace 7 --boot 4242-1700000000";
pub(super) const RENAME_HELP: &str =
    "--boot BOOT says which run of the server the id is from: `boot` in `status --json`, or in a machine's
entry there. Ids last one run, so a server that restarted since refuses, nothing is renamed, and the
command prints why and exits with 4.

Examples:
  cornercase rename 'review #42'
  cornercase rename --workspace 5 ''
  cornercase --remote vps rename --workspace 7 docs --boot 4242-1700000000";
pub(super) const FOCUS_HELP: &str =
    "Shows it in the window, or with --view ID in that view only (`cornercase view`), which then
follows it: the window and other views stay where they are.

--reveal TEXT (a pane only) also scrolls the pane to the text and selects it, as Enter on a result of
`cornercase search` does: TEXT is what the search looked for, and --line is the match's line from
`search --json` for a screen match. Leave --line out for an agent's prompt or reply, which takes the
newest place the pane shows the text. The scroll and the selection are the pane's, so the window and
every view show them. When the pane no longer holds the text it is still shown, a warning goes to
stderr and the exit status is 0.

--machine NAME picks one of a machine's workspaces or tabs as the one shown: --workspace and --tab
take the ids `cornercase status` lists under that machine. The machine must have sent its workspaces
since this server started. The window then shows that tab, or the workspace's active tab, streamed
from the machine, and `status --json` reports it as `shown_remote`, with its stream's state.

--json prints the answer: `revealed` with --reveal, and with --view the view's new tab and its
`projection`, the number a view's lines and surface frames carry, which goes up each time what the
view shows changes. With --machine it is what was picked, as `shown_remote` in `status --json`.

Examples:
  cornercase focus --pane 12
  cornercase focus --pane 12 --reveal 'migration failed' --line 340
  cornercase focus --view 41 --tab 7
  cornercase focus --machine vps --workspace 4";
pub(super) const GO_HELP: &str =
    "Does what a click on an agent's desktop notification does, or the go button of its toast: shows
the agent's pane (the one still needing you, when the notice covered several), or its workspace once
they are all closed, and prints the pane's id. With --view ID it moves that view (see
`cornercase view`) instead of the window; a notice about an agent on one of your machines refuses
--view, since only the window shows that machine's workspace. TOKEN is from `cornercase events --notices`.
It fails for a token this server never gave or no longer keeps (the last 64; a restart forgets them),
and when the agent and its workspace are gone.

Examples:
  cornercase events --notices --json | jq -r 'select(.event == \"notice\") | .token'
  cornercase go cc-3f2a9c-4
  cornercase go cc-3f2a9c-4 --view 41";
pub(super) const NOTIFY_HELP: &str =
    "The desktop notification goes through the terminal of every open window, like the one for an
agent that needs you. A program that follows `cornercase events --notices` gets it too, as a message
of kind notify that names the pane the command ran in.

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
opencode, Grok Build, Cursor CLI, Vibe CLI and GitHub Copilot CLI, the same records as `read --last-message`) and in the history of every
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

--boot BOOT says which run of the server the pane or tab id is from: `boot` in `status --json`, or in
a machine's entry there. Ids last one run, so a server that restarted since refuses, nothing is
marked, and the command prints why and exits with 4.

Examples:
  cornercase mark
  cornercase mark --pane 12
  cornercase mark --pane 12 --reviewed
  cornercase --remote vps mark --pane 9 --boot 4242-1700000000";
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
An agent on a linked machine tells its notices too, with the machine's name and without ids here:
  machine vps  notice  claude on vps needs you in shop › main  cc-3f2a9c-5
`cornercase go TOKEN` then does what a click on that notification does.
The server's other notices come with --notices too, as the window's toast says them, with a kind and
no token: notify (`cornercase notify`, naming the pane it ran in), queue-started, queue-failed,
queue-missed and queue-kept (a queued agent or routine run; a one-off entry that missed its time sends
none), setup-failed (only while nobody looks at its tab), flow-consent (a flow's member waits for your
consent) and flow-merged (a run's agents closed once its pull request merged). With --json they have
kind, message, panes and the workspace's ids, which queue-failed and queue-missed leave out:
  notify  the release build is ready
  workspace 2  flow-merged  pull request merged in fix-login: 3 agents closed · ▶ on the workspace reopens them
With --host, a program's own requests to the terminal are told too, for programs that draw the panes:
  pane 7  clipboard  11 bytes         it wrote to the clipboard (OSC 52); the text itself only with --json
  pane 7  bell  3 times               it rang the bell: one line a second per pane, with how many rang
With --json, each line is a JSON object with the event, the ids, the old and new state and the time.

It ends when the server stops, once every pane given with --pane has closed, or soon after what reads
its output is gone (a closed pipe or socket, such as an ssh that ended), with status 0. Programs are looked
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
which rows go on in the next one (wrapped), cursor, modes and scroll position, plus the split tree,
the dividers, and boot, session and revision.

Each line names its stream with `surface`, which `cornercase resize` takes to lay a --tab surface out at
another size. With --tab the surface resizes no terminal: terminals keep the size of whoever sizes the tab (the
window, or a view), so size can differ from rect. With --view ID it streams the tab that view
shows, at the view's size, and follows the view to other tabs (each line carries view and
projection); it pauses while the view is inactive or has no size, and ends when the view closes.
The window keeps showing what it showed, also when the tab is not the one on screen. It ends when the tab closes, the
server stops or what reads it is gone (status 0); stopping it leaves the shells running. A reader that falls behind gets
fewer lines, each complete.

With --popup it streams the plugin popup open in the window (see `cornercase plugin run`) instead of a tab:
one pane, its program's terminal, at the popup's own size, with `popup` naming the plugin and the action,
`tab` 0 and no layout. It ends when the popup closes. `cornercase input --popup` types into it.

With --patches, each line after the first carries only what changed since the line before: `base` is the
revision it applies to, and each changed pane has its cursor and what changed of the rest (`lines` by
row number, `copy` for rows that moved, `wrapped` whole). A complete line, without `base`, comes again
whenever the layout, a size, the panes or the view change, after the reader fell behind, and when
`cornercase redraw --surface ID` asks for one.

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

--boot BOOT says which run of the server the pane id (with --popup, the popup's id) is from: `boot` in
`status --json`, or in a machine's entry there. Ids last one run, so a server that restarted since
refuses, nothing is typed, and the command prints why and exits with 4. --follow does not take it.

With --follow it keeps running and reads one event per line from standard input (give - as the
only EVENT) until it ends: each line goes as soon as it arrives, and the lines that arrive while
the answer before is on its way go together, so a program types through one process (and with
--remote one ssh connection). A line may name its pane, {\"pane\": 14, \"key\": {\"code\": \"enter\"}},
else it goes to --pane. Each request is all or nothing on its own: with --json it prints one line
each, the answer with events (how many lines it answers) or its error, and a line that is wrong or
refused does not stop the next ones; the command then exits with 3.

Examples:
  cornercase input --pane 12 --follow --json -
  cornercase input --pane 12 '{\"text\": \"ls\"}' '{\"key\": {\"code\": \"enter\"}}'
  cornercase input --pane 12 '{\"mouse\": {\"kind\": \"scroll-up\", \"x\": 10, \"y\": 5}}'
  cornercase input --popup --pane 40 '{\"key\": {\"code\": \"down\"}}' '{\"key\": {\"code\": \"enter\"}}'
  cornercase --remote vps input --pane 9 '{\"paste\": \"two\\nlines\"}' --boot 4242-1700000000";
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

With --json it also lists `agents`: each agent the `agent` setting can name, with its title, whether
cornercase reads its status (`reads_status`: a routine can remove the workspace of those only),
whether a flow can run it (`flows`), the `models` and `efforts` the flow editor offers for it at once,
and whether it keeps more models in a list of its own (`catalogue`), which `config models AGENT` reads.

`models AGENT` prints every model the flow editor offers for an agent, one per line and nothing when
it offers none: its `models`, then, when `catalogue` is true (opencode: the models.dev list it
caches), that list, which the server reads when asked. --json prints the agent and its models. An
agent the `agent` setting cannot name is refused.

`set` changes settings with the dialog's own checks and applies them at once, as the dialog does;
nothing is saved unless every value is accepted. Values are converted by the setting's type: true or
false for a switch, a whole number, a comma-separated list for issue_tabs, otherwise text.

A token is never an argument: `secret` reads it from standard input (hidden on a terminal), checks it
with its service as the dialog does, and says which account accepted it, never the token. `remove`
forgets a token, a bot, a paired chat or the paired browsers. `pair` connects a phone to a chat or a
browser: `cornercase config pair --help`.

`forum-server` picks the server of the Discord forum, when forum mode waits for one and the app is in
several: `status --app` lists them as `servers`, in Discord's `forum`, and this is a click on one in the
forum's dialog. It answers once the pick is taken: the channel is made next, and `status --app --follow`
goes on to `ready`. An id that is not listed, or a moment when no pick waits, is refused and says why.

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
  cornercase config models opencode --json
  cornercase config set sidebar tree
  cornercase config set fetch_minutes 10 counts false
  cornercase config set issue_tabs github,linear
  printf '%s' \"$LINEAR_API_KEY\" | cornercase config secret linear
  cornercase config remove linear
  cornercase config pair browsers --json
  cornercase config forum-server 1041234567890123459
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

To work on one: `cornercase start --issue KEY`, or `cornercase flow run NAME --issue KEY` (`--prompt` gives the team your own task).

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

pub(super) const REMOTE_ARG: &str = "Run the command against the cornercase of another machine, through ssh: a host from \
    ~/.ssh/config, or user@host (or set CORNERCASE_REMOTE)";
pub(super) const COMMAND_THERE: &str = "How to run cornercase there, as sh reads it, or on a Windows machine its path (~\\ for \
    the profile folder) [default: cornercase, also looked for in ~/.local/bin, ~/.cargo/bin and Homebrew's folders]";
pub(super) const REMOTE_HELP: &str =
    "cornercase runs `ssh DESTINATION cornercase proxy`, so your ssh config, keys, agent and
ProxyJump apply, and starts the server there if none is running. The shells, agents and files stay
on that machine; this one only shows them, and copies and notifications go through this terminal.
If the connection drops, the window stays and reconnects by itself; Esc or quit gives up. Both
machines need the same version of cornercase. Reconnecting never asks for a password, so use a
key or an agent. To send the commands for scripts and agents there, give them --remote DESTINATION.
The other machine may run Windows: the first time, cornercase finds out whether its sshd runs
commands with cmd.exe or PowerShell, and remembers it.

--known prints the machines a window on this machine offers in its machine button, in the order
its menu lists them: those a window reached, with `cornercase remote` or the button, and those a
program added with --remember, each with its --command, and opens no window. With --json:
{\"machines\": [{\"destination\", \"command\", \"unreached\"}]}, command being null without one, and
unreached true for a machine added with --remember that no window has reached yet (left out
otherwise). It reads this machine's list, so it needs no server and no ssh, and CORNERCASE_REMOTE
changes nothing for it: a program that offers to move to another server reads it this way rather
than the file, whose shape can change.

--remember DESTINATION adds a machine to that list, with --command if given, and --forget
DESTINATION takes one off, as the button's forget… does. Like --known they open no window, need
no server and no ssh, and CORNERCASE_REMOTE changes nothing for them. A window asks before it
first reaches a machine added this way, then stops asking. Remembering a listed machine again
replaces its --command (none clears it). A window already open sees the change at its next
connection or forget, or when it opens again; one on that machine writes it back, with the
--command it reached it with, when it reconnects. DESTINATION is one word of at most 255 bytes that does not start with -;
another exits with 2. A machine's host key must be known to ssh, since a window never asks for it: accept it with a plain
ssh DESTINATION, or run cornercase remote DESTINATION with the same --command (without it, the saved
command is cleared). A machines.json that exists but cannot be read is refused with 1 and left alone. --forget of a machine that is not listed writes nothing and exits with 1.

Examples:
  cornercase remote devbox
  cornercase remote me@10.0.0.5 --command '~/bin/cornercase'
  cornercase remote me@winbox --command 'C:\\tools\\cornercase.exe'
  cornercase remote --known --json
  cornercase remote --remember me@10.0.0.5 --command '~/bin/cornercase'
  cornercase remote --forget devbox";
pub(super) const LOGS_HELP: &str =
    "The server writes one line per event: the time in UTC, the level, where it happened and what,
then key=value details. It says which windows attached, which commands ran and how they were
answered, what started and stopped, how long background jobs took, what each agent was doing and
each step of starting one. It never holds what is typed, pasted or shown in a pane, nor tokens.

The log is server.log in the state folder ($XDG_STATE_HOME/cornercase, else ~/.local/state/cornercase),
or next to the socket when CORNERCASE_SOCKET is set. Past 8 MiB it moves to server.log.1 and starts
over. Start the server with CORNERCASE_LOG=debug for more: every event it handled, every git run,
slow frames.

With --remote DESTINATION it prints that machine's log, through ssh.

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
inside cornercase, say which with CORNERCASE_SOCKET=/absolute/path/server.sock. With --remote DESTINATION, or
CORNERCASE_REMOTE=DESTINATION, it runs on that machine, through ssh.

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
as it does when the command is interrupted. With --remote DESTINATION it runs on that machine, through ssh.

Examples:
  cornercase restart
  cornercase restart --when-idle --yes --timeout 3600";
pub(super) const UPDATE_HELP: &str =
    "With --when-idle, it installs the release at once, then restarts the server on it once no agent
is working, as `cornercase restart --when-idle` does. With --remote DESTINATION it runs on that machine, through ssh.

When this cornercase cannot replace itself (Homebrew installed it, its folder cannot be written, an unknown platform), it
changes nothing, prints the command to run instead and exits with 6; --check exits with 0 whatever it finds. --json prints
one line of JSON instead, asks nothing and never restarts the server (run `cornercase restart` after it):
{\"outcome\": \"updated\", \"from\", \"version\", \"path\"}, {\"outcome\": \"up-to-date\", \"version\"} or
{\"outcome\": \"manual\", \"version\", \"latest\", \"command\"}, and with --check \"available\" (with \"latest\" and
\"path\") where it would install. version is the binary on disk afterwards, on the machine it ran on. A failure prints
nothing on stdout and exits with 1.

Examples:
  cornercase update
  cornercase update --when-idle --yes
  cornercase update --json";
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
pub(super) const FRESH_HELP: &str = "Make a missing branch from the up-to-date default branch, as a routine's run does: fetch the remote first (origin, else the first one) and fail with git's reason if that fails [default: settings → worktrees → new branches]";
pub(super) const NO_FRESH_HELP: &str =
    "Make a missing branch from the project's HEAD, whatever settings → worktrees → new branches says";
pub(super) const QUEUE_FRESH_HELP: &str = "Make a missing branch from the up-to-date default branch, as a routine's run does: fetch the remote first (origin, else the first one) and fail with git's reason if that fails [default: settings → worktrees → new branches when the entry runs; edit keeps the entry's]";
pub(super) const BOOT_ARG: &str = "Act only if the server is still this run of it, the `boot` of `status --json`; else nothing is done and the exit status is 4 [default: any run]";

pub(super) const CONTRACT_HELP: &str =
    "Programs that drive cornercase without this command line (an editor extension, a dashboard) ask
the server for it before anything else. `contract` is the version of the requests and answers: it
goes up only when one changes in a way an older client would misread, while new commands, fields
and events are added without it. `boot` names this run of the server and changes on every restart,
since ids last only one run; close, rename, mark, move-pane, collapse, expand, account restart, context,
send, input and paste-file take it as --boot, and then act only in that run. Each command is listed
with whether it is available, and each stream with its version. A server without this command is older than the
contract.

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

--boot BOOT says which run of the server the pane ids are from: `boot` in `status --json`, or in a
machine's entry there. Ids last one run, so a server that restarted since refuses, nothing moves,
and the command prints why and exits with 4.

Examples:
  cornercase move-pane --pane 4 --target 7 --place swap
  cornercase move-pane --pane 4 --target 7 --place below --json
  cornercase --remote vps move-pane --pane 9 --target 3 --place left --boot 4242-1700000000";
pub(super) const FOLD_HELP: &str =
    "Sets it, whatever it was: a group shows or hides its projects, a project its workspaces and a
workspace its tabs; an agent's pane (--pane) hides the agents it started in the agents section
for as long as the agent running there now keeps running (another conversation there starts expanded).
Its shells keep running.

--boot BOOT says which run of the server the id is from: `boot` in `status --json`, or in a machine's
entry there. Ids last one run, so a server that restarted since refuses, nothing is folded or
unfolded, and the command prints why and exits with 4.

Examples:
  cornercase collapse --group 12
  cornercase collapse --pane 12
  cornercase expand --worktree fix/login
  cornercase --remote vps collapse --pane 9 --boot 4242-1700000000";
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

mod changes;
mod clone;
mod context;
mod files;
mod flow;
mod held;
mod machines;
mod open;
mod pair;
mod paste_file;
mod plugin;
mod plugin_windows;
mod ports;
mod processes;
mod pull_request;
mod recap;
mod requests;
mod setup;
mod start;
mod status;
mod tunnels;
mod worktrees;

pub(in crate::cli) use changes::{CHANGES_FILE_HELP, CHANGES_GAP_HELP};
pub(in crate::cli) use clone::CLONE_HELP;
pub(in crate::cli) use context::CONTEXT_HELP;
pub(in crate::cli) use files::FILES_HELP;
pub(in crate::cli) use flow::{
    FLOW_CLOSE_WHEN_MERGED_HELP, FLOW_DEFAULT_HELP, FLOW_DELETE_HELP, FLOW_EXPORT_HELP, FLOW_FILES_HELP, FLOW_HELP,
    FLOW_HIDE_HELP, FLOW_IMPORT_HELP, FLOW_MESSAGES_HELP, FLOW_ORDER_HELP, FLOW_RESTORE_HELP, FLOW_RUN_HELP,
    FLOW_RUNS_HELP, FLOW_SAVE_HELP,
};
pub(in crate::cli) use held::HELD_HELP;
pub(in crate::cli) use machines::{MACHINES_ADD_HELP, MACHINES_HELP, MACHINES_UPDATE_HELP};
pub(in crate::cli) use open::OPEN_HELP;
pub(in crate::cli) use pair::CONFIG_PAIR_HELP;
pub(in crate::cli) use paste_file::PASTE_FILE_HELP;
pub(in crate::cli) use plugin::PLUGIN_HELP;
pub(in crate::cli) use plugin_windows::PLUGIN_WINDOWS_HELP;
pub(in crate::cli) use ports::{PORTS_HELP, PORTS_STOP_ABOUT, PORTS_STOP_HELP};
pub(in crate::cli) use processes::{PROCESSES_HELP, PROCESSES_STOP_ABOUT, PROCESSES_STOP_HELP};
pub(in crate::cli) use pull_request::PULL_REQUEST_HELP;
pub(in crate::cli) use recap::RECAP_HELP;
pub(in crate::cli) use requests::REQUESTS_HELP;
pub(in crate::cli) use setup::SETUP_HELP;
pub(in crate::cli) use start::{NEW_WORKSPACE_HELP, START_HELP};
pub(in crate::cli) use status::STATUS_HELP;
pub(in crate::cli) use tunnels::{TUNNEL_HELP, TUNNELS_HELP};
pub(in crate::cli) use worktrees::{
    WORKTREES_FREE_HELP, WORKTREES_HELP, WORKTREES_LIST_HELP, WORKTREES_REMOVE_HELP, WORKTREES_SIZE_HELP,
};

#[cfg(test)]
mod tests;

pub(super) const VIEW_ARG: &str = "Act in this view (see `cornercase view`): its tab is the default, and --focus moves the view instead of the window";
pub(super) const VIEW_HELP: &str =
    "For programs that draw cornercase themselves, such as an editor extension or a GUI, so that each
picks its own tab and size without touching the window or the others. It keeps running and prints
the session as one JSON line each time it changes, like `status --follow`, with a `view` object:
id, projection, active, cols, rows, shown (the project, workspace, tab and pane it shows) and
controls (whether it sizes its tab's terminals now). The view lives while this command runs:
stopping it closes the view and gives its tabs back. The first line is the state now; the view's id
is in it. Keep it running in the background (`cornercase view --cols 120 --rows 40 > view.jsonl &`),
read the id from the first line, and pass it as --view ID to `focus`, `open`, `clone`, `new-workspace`,
`new-tab`, `split`, `setup`, `start`, `flow run`, `send`, `keys`, `held`, `read`, `wait`, `status`, `rename`,
`mark`, `go`, `plugin run`, `surface`, `account add --login` or `account login`: the view's tab is
their default and --focus moves it.

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
--visible says it is back, which also reads the open pull requests again (not those read in the last
10 seconds). It prints the view as it is now.

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

pub(crate) const ISSUE_LIST_HELP: &str =
    "A tracker that fails does not stop the others: it is named on stderr with its reason, its cached
list is still printed (with --json, its entry in sources has an error), and the command exits 3
after listing the others.";

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
asked, after printing them all, that one with its error. --machine NAME asks that machine over ssh
and adds \"machine\" (name, state, seen, read_at, error); --machine all prints this computer and
then each machine under \"machines\", and exits with 3 when a machine could not be read either.

Examples:
  cornercase usage
  cornercase usage --cached --json
  cornercase usage --machine vps
  cornercase usage --machine all --json";

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
instructions, skills); `account list` names the ones that stopped being links. `account add
--login` opens a tab that logs the new account in; when a login expires (`account list --check`
says signed out), `account login AGENT ACCOUNT`, or --system for the agent's own login, opens that
tab again. Gemini CLI has no login command: start it in a tab of that account and sign in from its
first screen. Removing an account deletes its folder, never the system's keychain, and an agent
that used it goes back to its own login.

account restart takes --boot BOOT, the `boot` of the `status --json` its pane ids come from: a
server that restarted since refuses, nothing restarts, and it exits with 4.

Examples:
  cornercase account add claude work --login
  cornercase account login claude work --focus
  cornercase --remote vps account login codex --system --json
  cornercase account use claude work
  cornercase account restart claude --pane 4
  cornercase --remote vps account restart claude work --pane 9 --boot 4242-1700000000
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
