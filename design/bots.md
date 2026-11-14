# Bots

Code: `bots/` (`mod.rs`, `brief.rs`, `chat.rs`, `folder.rs`, `turn.rs`), `agents/headless.rs`, `agents/{claude,codex}/headless.rs`, `agents/probe.rs`, `bots/form.rs`, `app/bots.rs`, `app/bots/` (`turns.rs`, `manage.rs`, `shown.rs`, `window.rs`, `section.rs`, `talk.rs`, `menu.rs`, `form.rs`), `ui/bots.rs`, `ui/bots/`, `ui/dialog/bot.rs`, `app/control/bots.rs`, `control/bots.rs`, `cli/bot.rs`, `cli/bot/`, `cli/args/bot.rs`, `cli/help/bot.rs`, `cli/guard.rs`, `paths.rs` (#844, #845), `app/control/boundary.rs`, `os/socket.rs`, `os/process.rs` (`Lineage`), `processes/stop.rs`, `app/launches.rs` (#856)

A **bot** is an agent of your own: a name, a folder, a memory and a chat. It belongs to no project, but it can see all of them and act on them through the `cornercase` command line. It exists for work that belongs to no project, or to all of them ("review my open pull requests every morning", "tell me which issues labelled `bug` came in today, in every project"), which today meant opening a folder as a fake project, or keeping an agent alive in a pane whose context grows for ever.

This is the base of the bots series (#844 to #849). It holds the model, the folders, the conversation, the shared runner, the commands, the requests and the events. The window draws them since #845 (see [The window](#the-window-845)), there are no routines (#846), no following of the agents a bot starts (#847), no chat apps (#848), and only Claude Code and Codex can be bots (#849 adds the others).

**Making one starts nothing and spends no tokens.** A bot runs only when someone writes to it, and only while the server runs: a turn's process does not survive the server.

## Folders and files

`bots/` beside the session (`paths::bots(session)`, through `beside_session` as `journal` and `recap` are: `x.bots/` for `x.json`), so a dev server of another tree and a session with another file name each have their own. One folder per bot, `bots/<id>/`, 0700 on Unix:

- `bot.json` (0600, written through `persist`): `id` (a slug of the name, unique, kept when the bot is renamed), `name` (1 to 40 characters, unique ignoring case, no control characters), `agent`, `model`, `effort`, `account`, `access` (`cornercase` or `full`), `created` (UTC seconds) and `conversation` (the agent's session id for the chat; none before the first message and after `reset`).
- `bot.md` (0600): what the bot is for. Created from `--about` (may be empty); the user and the bot both write it, and the brief tells the bot to keep it up to date.
- `memory.md` (0600): what it keeps from one conversation to the next. Created empty; the bot adds to it, keeps it short and prunes it.
- `chat.jsonl` (0600): the chat, append-only.

At most 50 bots (`MAX_BOTS`). A folder whose `bot.json` does not parse, or whose `id` is not the folder's name, is skipped when loading, but its name still counts as taken for a new bot's id, so a new bot never overwrites it. `remove` deletes the folder before it forgets the bot, so a folder that cannot be deleted leaves the bot where it was.

**The server never re-reads `bot.json` while it runs**: memory is the truth, and the server rewrites the file when it changes a bot. A bot's access lets it write in its own folder, and `bot.json` holds its `access`, so re-reading it would let a bot give itself full access. Claude's denied list also names `bot.json` and `chat.jsonl` (see Access). Codex's sandbox cannot exclude one file, so a Codex bot could rewrite `bot.json` for the next server start; this is not a real boundary, since a `cornercase` access already lets a bot start agents with `cornercase start`, and Codex bots are `full` only today anyway.

## The chat

`chat.jsonl` is cornercase's chat, not the agent's transcript. That way a routine's report (#846) and a notice about an agent the bot started (#847) can land in the same chat, and the chat survives `reset` and a change of model. One JSON object per line, never rewritten while the server appends to it (the same rule as the journal's day files, see Recap).

| `kind` | what it holds |
|---|---|
| `you` | the text the user sent; `held: true` when a turn was running |
| `bot` | the agent's final answer of a turn, in Markdown, with `model`, `tokens` and `conversation` |
| `step` | one tool the agent used, as one line (`cornercase status --json`, `Read src/main.rs`), with its `tool` |
| `error` | why a turn failed, in the agent's own words when it gave any |
| `notice` | cornercase's own lines: `new conversation`, `stopped`, `interrupted by a restart`, `1 held message dropped` / `N held messages dropped` |

Every line has `id` (a number that increases per bot) and `at` (UTC seconds). Absent optional fields are not written. The notice texts are fixed and part of the contract.

- **A torn last line** (the server stopped mid-write) is skipped by readers, and healed at load: if the file does not end with a newline, one is appended before the next line, so the next line parses.
- **Replay is the source of truth at load** (`chat::replay`): a `you` that is not held starts a turn that runs; held ones are counted; a `bot` or `error` ends the turn, and held messages start the next one at once; `stopped` and `interrupted by a restart` leave it idle with nothing held. No extra field is kept for the turn. A turn still running in the replay gets `interrupted by a restart`, and held messages get the dropped notice, once (a second load finds the notice and appends nothing).

## A turn

A turn is one run of the agent without its interface, through the shared runner (below), in a conversation that continues.

- **The first message of a conversation starts a new session.** Its opening message is the brief, then `bot.md`, then `memory.md` (each cut to 8 KB on a character boundary, saying it was cut; an empty file reads `(empty)`), then what the user wrote. Later messages send the text alone and continue the session: Claude with `--resume <session>`, Codex with `codex exec … resume <session>`. The session id comes from the run's own output (Claude's `system/init` and `result` lines carry `session_id`; Codex's `thread.started` carries `thread_id`) and goes into `bot.json`.
- **The session id is kept only on success.** A first turn that fails, is stopped or times out leaves `conversation` empty, so the next message opens with the brief again; a later one that fails keeps the session, so the next message continues it.
- **One turn at a time per bot.** A message sent while a turn runs is held and goes as the next turn, joined with the others held by a blank line. Its `you` line is written at once with `held: true`. Different bots run at the same time.
- **Steps stream.** Each tool use read from the run's output is written as a `step` line and published as it happens, before the `bot` line.
- **`stop`** ends the running turn: the process and what it started (`processes::stop`, as the Processes dialog stops a tree: TERM, a grace, then KILL). It writes `stopped`, keeps the session id, and **drops the held messages** with a notice, so a stopped bot is idle. A stop that arrives before the process exists still ends it (`probe::Running` remembers it).
- **A contained turn stops what it started whenever it ends** (#856): the moment the answer is read (a turn closes the root's stdin at the start, but the root, such as Claude Code, usually still runs until it has printed its result, so its children, orphans it adopted included, can still be listed), the root's children are captured, the root is given its grace and then killed, and the captured trees are stopped; a root that already exited by then leaves nothing to list (`processes::stop::below`, `stop_all`: TERM, a grace, KILL). Without it a command a bot left running could connect after its turn, when no turn holds it.
- **Time limit: 15 minutes per turn** (`TURN_TIMEOUT`; the app keeps it in `Bots::timeout` so tests shorten it, as Recap does with `RECAP_TIMEOUT`). A bot acts by starting agents and returning; it does not do long work itself. The timeout ends the whole process tree, and the `error` line says `claude did not answer in time`.
- **A restart** drops a turn's process with the server: see Replay above.
- **A second `stop`** adds what was held since to the ones to drop and starts no second job, so a `say --wait` held in between is answered with the dropped notice. **The server stopping** (`stop_background`, so `kill-server`, `restart` and an update) ends every running turn's process tree, one thread each, so the agent does not outlive the server it reports to. A message held while a stop was in flight runs as the next turn: replay keeps `held` at `stopped`, takes off the count a following `N held messages dropped` names, and reads what is left as the turn that runs.
- **`reset` and `remove` refuse while a turn runs** (`stop it first`), the same way: forgetting a session a process still uses, or deleting its folder under it, would leave the chat saying something false.
- **`reset`** (the window's ` new conversation `) forgets the session id and writes `new conversation`. `bot.md` and `memory.md` stay, so the next message starts a new session that reads them.
- **Failures keep the agent's words** (`Not logged in`, `usage limit`). Two are worded by cornercase: `error_max_turns` becomes `claude stopped after 40 steps, its limit for one message`, and a failure whose words name the context (`prompt is too long`, `context window`, `context length`, `maximum context`, `context_length_exceeded`) gets `; start a new conversation: cornercase bot reset <id>`.
- **Unread**: each bot counts the `bot` and `error` lines since a client last showed its chat (`bot seen`). It lives in memory, so a restart shows nothing unread.
- **Notices**: a `bot` or `error` line makes a toast (` <name>: <first line> ` with a ` go ` that shows the chat) and a desktop notification, whose click shows it too. A turn that a `say --wait` client waits on counts as shown, and so does one answered while a window shows the chat: it neither notifies nor counts as unread. Everything else does. Chat apps are #848.

## The brief

What cornercase tells the bot about itself. The server writes it from Rust constants (`bots/brief.rs`, one per section) and sends it in the opening message of every conversation. It is never written to the bot's folder, so a new release updates every bot at its next conversation. It says who the bot is and that it answers briefly, in the user's language, in Markdown; where its folder is and what `bot.md` and `memory.md` are for; how to see the user's work (`cornercase status --json`, `search`, `recap`, `worktrees list`, `read --pane ID`, read-only `git` and `gh`); how to act (`cornercase start --project ID …`, `send --pane ID`, `notify`, and that it must always name the project, since it runs in no pane); and what its access allows. A test runs `--help` for every command the brief names, so it cannot name a command that does not exist.

Rejected:

- **Writing the brief into the folder as `AGENTS.md` or `CLAUDE.md`.** The agent would load it on its own, a second time, and an old copy would outlive an update.
- **The agents' system prompt flags.** `claude --append-system-prompt` exists and survives compaction, but Codex's `exec` has no stable equivalent, and one rule for both agents is easier to test with the fakes. A conversation that compacts may lose the brief; `reset` brings it back.

## Access

A turn runs without its interface, so the agent cannot stop halfway to ask for permission. What it may do is fixed when the bot is made (and changed by `bot edit` from the next turn; never by the bot itself, see the guard).

- **`cornercase` (the default)**: it reads anything, runs the `cornercase` command line and read-only git and gh, searches the web, and writes only in its own folder.
  - Claude: `claude -p --setting-sources user --output-format stream-json --verbose --max-turns 40 --permission-mode dontAsk --allowedTools Read,Grep,Glob,WebSearch,WebFetch,Bash(cornercase:*),Bash(git log:*),Bash(git show:*),Bash(git diff:*),Bash(git status:*),Bash(gh pr list:*),Bash(gh pr view:*),Bash(gh pr checks:*),Bash(gh issue list:*),Bash(gh issue view:*),Edit(<folder>/**) --disallowedTools NotebookEdit,Task,Bash(cornercase kill-server:*),Bash(cornercase restart:*),Bash(cornercase update:*),Edit(<folder>/bot.json),Edit(<folder>/chat.jsonl) [--resume S] [--model M] [--effort E] -- <prompt>`. `--permission-mode dontAsk` denies what is not allowed instead of asking, and stops the user's own `defaultMode` (`auto`, `acceptEdits`, even `bypassPermissions`), which `--setting-sources user` loads, from applying.
  - Codex: `codex exec --json --skip-git-repo-check --sandbox workspace-write -C <folder> [-m M] [-c model_reasoning_effort=E] [resume S] -- <prompt>`, so it writes only in its folder. See the live checks: this access is not offered.
- **`full`**: Claude with `--permission-mode bypassPermissions` (only the commands that stop the server are denied), Codex with `--dangerously-bypass-approvals-and-sandbox`. `bot new --access full` and the form say what it means: it can change any file and run anything, unattended. **Off by default, never chosen for the user**: an agent that supports `full` only (Codex) is refused without `--access full`, with that warning.
- **Settings**: neither agent loads a project's settings, since there is no project. Claude loads the user's settings (`--setting-sources user`), so the user's MCP servers and `~/.claude/CLAUDE.md` work, unlike Recap's empty sources: a bot is the user's own assistant. No `--strict-mcp-config` and no `--no-session-persistence` (the session must be resumable). A Codex bot reads the user's own `~/.codex/config.toml` as it does without cornercase (no `--ignore-user-config`).
- **What `bot agents` says**: `bot: false` with `why` (`not yet` for an agent without a headless spec, `not installed`), or `bot: true` with the `accesses` it supports and, when the access is limited, `why`: `its sandbox cannot reach cornercase` (Linux, checked) or `its sandbox is not known to reach cornercase on this system` (macOS and Windows, not checked). A UI builds its form from this list, so #849's agents appear without changes elsewhere.

## The environment of a turn

- **Working folder**: the bot's folder.
- **`CORNERCASE_BOT=<id>`** (`bots::BOT_ENV`). The command line reads it (see the guard), and so does the server, as a fallback to tell a bot's commands (The boundary).
- **`PATH`** starts with the folder of the server's own binary (`process::program::own_binary`, `std::env::join_paths`), so `cornercase` is the binary of the server that runs the bot. This matters for a dev build and on Windows (`cornercase.exe`). Codex runs its commands through a login shell (`zsh -lc`), which re-prepends the user's own folders: its `cornercase` may be the installed one rather than this server's, another reason Codex stays `full` only.
- **`CORNERCASE_SOCKET`** is set from `App::shells.pane_socket()` when there is one (a dev build), as Recap's `Ask::socket` is; otherwise the child inherits the server's environment, and a release server's binary finds the default socket. (The issue named `CORNERCASE_SERVER`, but that variable is the boot token that validates `CORNERCASE_PANE`, not the socket.) `CORNERCASE_PANE`, `CORNERCASE_SERVER` and `CORNERCASE_REMOTE` are removed: there is no pane.
- **A `cornercase`-access turn runs contained** (#856): on Linux the turn's root is a child subreaper, so what a command sends to the background and orphans is adopted by it; on Windows the root is in a Job object (`KILL_ON_JOB_CLOSE`, no breakaway). The server tells a bot's commands by this (The boundary), and what a contained turn started is stopped when it ends. A `full` turn, Recap and the usage probes are not contained.
- **Account**: as Recap's `Call::home` (the account's folder in the agent's environment variable, and the variables it lists removed), through `App::account_home`, which Recap now shares.

## The shared runner

`agents/headless.rs` is **one runner for Recap and bots** (it was `agents/recap.rs`): `Ask`, `Ran`, `Spec`, `Call`, `run`, with each agent's `headless.rs` holding its arguments and its reader. `Known::headless()` replaces `Known::recap()`; an agent without it can be neither. What bots added on top, none of which changes Recap's arguments (two exact-argv tests, written before the move, pin them):

- `Ask::tools` (`Tools::Recap` or `Tools::Bot(Access)`) picks the arguments; `Ask::conversation` continues a session; `Ask::env` sets (`Some`) or removes (`None`) variables.
- `Ran::session` (the session id out) and `Ran::fresh` (the tools used since the last line, which `run_watched` hands to `Watch::steps` as they arrive). `Action` is a tool and its one-line summary (Bash: the command's first line; Read, Edit and Write: the path; Grep and Glob: the pattern; WebSearch, WebFetch: the query or URL).
- `run_watched(call, ask, Watch)` carries the progress counter, the steps callback and a `probe::Running`. `Spec::bot` says whether the agent can be a bot, with its accesses and why they are limited (`BotSupport`).
- **`probe::Running`** lets a bot's `stop` end a run, also before its process exists; a timeout ends the whole tree (see Plan usage › A timeout ends the whole tree).

## Requests and events

One operation, **`bot`**, whose `args` are `{"action", "entry"}` (`entry` is the action's fields, left out for `agents` and `list`) (`agents`, `new`, `list`, `edit`, `remove`, `reset`, `seen`, `stop`, `say`, `history`), adjacently tagged as `config`, `machines` and `queue` are (`{"command":"bot","args":{"action":"say","entry":{"bot":"prs","text":"hi","wait":true}}}`). `src/control.rs` was 989 lines and `tests/layers.rs` fails a file over 1,000, so ten request names with their lists did not fit; one name needs four lines, and every type lives in `control/bots.rs`. An older server refuses `bot` by name. All of it is behind the contract feature **`bots`**, which the CLI checks before any `bot` command (`the running cornercase server has no bots`). Request structs have no `skip_serializing_if`, so a strict client's unknown-field check works.

- `bot` names a bot by id, else by name ignoring case; an unknown one is `not-found`. Validation errors are `malformed` (a name that is empty, over 40 characters or has a control character; an unknown agent, model, effort, account or access; a message empty or over 20,000 characters, refused with its size rather than cut), `conflict` (a name taken; the 51st bot) or `busy` (`reset`, `remove` while a turn runs).
- `say` answers at once with the `you` line. With `wait` it answers when that message's turn ends with the `bot`, `error` or `notice` line (`Stage::Bot`, which does not outlive its client; the answers are kept per bot for the waited ids only, at most 32). The CLI exits 3 unless the line is a `bot` line.
- `edit` cannot change the agent (`a bot keeps its agent: another agent cannot continue its conversation; make another bot`); an empty `model`, `effort` or `account` clears it; a change applies from the next turn and publishes `change: edited`.
- **Events** (`cornercase events`): `{"event":"bot","bot":ID,"line":{…}}` for each chat line, steps included; `{"event":"bot","bot":ID,"state":"thinking|idle"}`; `{"event":"bot","bot":ID,"change":"created|edited|removed"}`. Exactly one of the three keys is set. They go to every reader that did not filter by `--pane` (`Event::kind()` is `None`), as Recap's do. A client that shows a chat reads `history` once and follows the events. The lines appended at load are not published: no reader is connected yet.
- **`status --app`** has a `bots` key (the same as `bot list --json`, left out when empty so the v1 snapshot fixture keeps its bytes), so a UI updates its list without polling. `unread` and `seen` travel through it: `seen` publishes no event.
- `--remote` works as for any command. A bot on another machine is left for later.

## The guard: `CORNERCASE_BOT`

A bot runs in no pane, so a command that would default to "the one this runs in, else the shown one" would act on whatever the window shows. When `CORNERCASE_BOT` is set and not empty, `cli::parse` computes a refusal from the matches it already has (`cli/guard.rs`) and `cli::run` returns it as wrong usage (exit 2) before doing anything:

1. `kill-server`, `restart` and `update`: `a bot cannot stop its own server`. This also covers Codex, whose sandbox cannot filter commands.
2. **A generic rule**: the deepest matched command has an argument whose help says `this runs in` (the marker shared by `HERE_PANE`, `HERE_WORKSPACE`, `BY_BRANCH`, `WAIT_PANE` and the literal defaults), and none of its place flags (`--workspace`, `--project`, `--pane`, `--tab`, `--group`, `--path`, `--all`, matched by their long name, since ids differ) was given: `a bot runs in no pane: name the place with --workspace ID or --project ID (cornercase status --json lists the ids)`. Nothing is listed by hand, so a new command whose help says `this runs in` is covered, and a test fails if someone rewords one of the four constants. **Known false positive**: `flow import` and `flow export` with `--to yours` and no `--project` are refused too; harmless, since a bot has no use for them.
3. `bot edit … --access` and `bot new … --access full`: `a bot cannot change a bot's access`.
4. `bot say` to its own id: `a bot cannot write to itself`, since a message held for itself would start another turn for ever. It matches the id in `CORNERCASE_BOT`; a bot that names itself by its name is not caught.

The command line's guard only prevents mistakes: what a bot's commands may type into, and what it may change about bots and settings, is enforced by the server (The boundary, below).

Without `CORNERCASE_BOT` (unset or empty) the command line's guard changes nothing; the server's boundary does not depend on it. #846 uses the variable as the default for `queue add --bot`, and #847 to tag what a bot starts.

## The boundary (#856)

Under `cornercase` access, **the server refuses what a bot types into a pane that runs no agent**: `send`, `todo send`, `keys`, `input` (a pane, and a plugin's popup always), `paste-file`, `held --send-now`, and `new-tab` or `split` with a command (their command is typed into the new shell). The same request into an agent's pane works, and so does the same request from any other client. The refusal has code `failed` (as `send`'s others), exit 1, and its text starts `a bot can only type into a pane that runs an agent` (`boundary::HEAD`).

- **Who is a bot is the connecting process, decided once per connection, at connect.** `Server::accept` reads the peer's pid from the socket (`os::socket::peer_pid`: `SO_PEERCRED` on Linux, `LOCAL_PEERPID` on macOS, `SIO_AF_UNIX_GETPEERPID` on Windows) and `App::client_connected` stores `client → bot id`. A connection a turn opened stays a bot's after its turn ends. **It fails closed**: while a contained turn runs, a connection whose peer pid cannot be read counts as that bot's (a real client is alive and readable while it waits for its answer). A pid that is read but whose process cannot then be inspected, and a Windows `Lineage` whose Job could not be created, fall back to the walk and the environment: a gap. A process is a bot's when a running contained turn holds it (`Running::holds`: its root, a descendant by the parent walk, or on Windows a process of the turn's Job), or when its own environment, read by the server from the OS, sets `CORNERCASE_BOT` to anything but a `full` bot's id (an unknown id counts: the check only ever refuses more). It is logged as `a bot's client`, with the bot's id and never a text.
- **Why contained turns.** Claude Code runs every Bash command in a session and process group of its own (checked live), and a command sent to the background with `( … &)` is reparented to pid 1 once its shell exits, so neither a group nor a plain parent walk reaches it. On Linux the turn's root is a child subreaper (`prctl(PR_SET_CHILD_SUBREAPER)` in `pre_exec`), so the orphan is adopted by it and the walk still finds it. On Windows nothing is reparented, so the root is in a Job (`os::process::Lineage`, `IsProcessInJob` asks), with no `BREAKAWAY_OK`: Cygwin and MSYS2, so Git Bash, add `CREATE_BREAKAWAY_FROM_JOB` when the job allows it. **macOS** has no subreaper: a command sent to the background whose shell exits is reparented to launchd and is not seen, unless it keeps `CORNERCASE_BOT` in its environment (the fallback above), and cleanup does not reach it either.
- **What "runs an agent" means** is the pane's foreground program now (`agents::detect` over `Term::foreground_args`, what the agent watch and `status`'s `program` use, so also an agent the user put in `agent_commands`), not `status`'s `agent`, which keeps an agent that exited back to its shell while its finished status is held. So a pane whose agent exited, or that is resuming its agent after a restart, is refused. A machine's pane is out of reach: no typing request takes `--machine`.
- **Where it runs.** The gate sits at the top of `handle_request` for a bot's client (`boundary::bot_writes`), resolving the target as the handler does. The writes that happen later are checked again when they happen, keyed on the pending request (kept when its client leaves, the keys in `Clients::bots` (`boundary::BotClients`)): a held send when it goes out (`held::release`), `paste-file` when its stored file is pasted (`file_stored`), and the Enter cornercase presses after `send --enter` (`drive_launches` ends that launch, answers the request with the refusal and leaves the pasted text unrun on the shell's line). `keys` and `input` write at once, so the race of keys, `input` or a `send`'s paste (without `--enter`) written in the instant an agent exits remains.
- **`full` bots are unchanged**: they can run anything themselves, so refusing them adds friction and no safety. Their turns are not contained.

**Also refused to a bot's connection** (it could lift the boundary itself): `bot new` with an access other than `cornercase` and `bot edit` with any access (`a bot cannot change a bot's access`), `bot say` to a bot with `full` access, and any `bot edit` of one, `--about` writing its `bot.md` included (`a bot cannot write to a bot with full access`), and every write to the settings (`config` other than get and search, `config-import` applied: `a bot cannot change cornercase's settings`), since `agent_commands` decides which programs count as an agent.

**What is still not a boundary**: the user's own Claude allow rules (`--setting-sources user` loads them on top of `--allowedTools`); an agent's own shell (Claude Code's `!` mode, which `send` and `keys` can type into), its permission prompts (which `keys` can answer), and starting agents in a mode that skips them; requests that change what cornercase runs (`flow-import`, `flow-save`, plugins); destructive requests (`close`, `stop-process`, `stop-port`, `changes-discard`, `worktrees-remove`); `--remote` over ssh (that machine's server sees an ssh process); and, on macOS, the orphan above. `cornercase requests` also lets a bot send a request the command line's guard refuses (`restart-when-idle`, `update-install`): now that the server knows a bot's connections it can refuse those too. Work you would give an assistant at your keyboard is the right size for a bot.

**Rejected**: a request field filled from `CORNERCASE_BOT`, or a token in the turn's environment (the bot controls its environment, and a token is readable by any process of the user); the turn's process group or session (every Bash command has its own); a socket per turn (the bot can name the main one); a wrapper process as the root (tests run turns with fakes, and macOS could still not adopt orphans); refusing a window's `Hello` from a bot's connection (a bot's commands have no terminal; a program that speaks the protocol needs the user's own allow rules); `status`'s `agent` as the test; a new error code (strict clients read `Code` as an enum); applying it to `full` bots.

**Checked live**: on Linux with Claude Code 2.1.325 (2026-11-14), every Bash command runs with `pgid = sid = ` its shell's pid, and a command sent to the background is reparented to pid 1 once that shell exits. Nothing was checked on macOS or Windows (the Job code was read against `windows-sys` 0.61.2 only), and no real turn ran: the tests use fakes.

## Limits and log

- A message is at most 20,000 characters.
- **Texts are never logged** (see Log): the log says `bot say bot=prs bytes=120` (and `about_bytes` for `new`), then, for a connection that is a bot's, `a bot's client` (`client`, `bot`), then the turn's agent, model, tokens, duration and how it ended (`answered`, `error`, `stopped`, `timeout`) under the target `bots`. The turn's job returns `Ok` whatever the agent said, so the job never logs an agent's text as its error.
- Not counted in Stats: the turn is no pane's agent, as with Recap.

## Checked live

On Linux, with Claude Code 2.1.325 and Codex 0.164.0 (macOS and Windows cannot be checked from here):

- **Claude's permission rules.** `--permission-mode dontAsk` is accepted and the run ends with a `result`; `--max-turns` is still accepted though `--help` no longer lists it. **A `Write(<path>)` rule does not apply** (denied for `Write(//abs/**)` and for `Write(./**)`), but **`Edit(<path>)` covers the Write tool too**: `Edit(//abs/**)` (and `Edit(./**)`, and the `///abs` spelling) allows a write inside the folder, a write outside is denied, and `--disallowedTools Edit(<folder>/bot.json)` blocks a write of that file though the folder is allowed. So the rules are `Edit` only, and the absolute path is `/` + the absolute path (`//home/u/…`; `rule_path`). On Windows the form `//c/Users/u/…` follows Claude Code's documentation and **is not checked**. `--resume <id>` remembers, and the session id is the same in `init` and `result`.
- **Codex's sandbox and the socket.** In `--sandbox workspace-write` a `cornercase` command cannot reach the server: `no cornercase server is running`, and with `-c sandbox_workspace_write.network_access=true` `Unknown error -1000`; with `--dangerously-bypass-approvals-and-sandbox` it works. So **a Codex bot cannot have `cornercase` access** on Linux: it is `full` only, `bot agents` says `its sandbox cannot reach cornercase`, and the form says so. On macOS and Windows the sandbox was not tried: they stay `full` only with the unverified wording, and the Known limitations list it.
- **Codex resume.** `codex exec --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox -C F [-m M] [-c …] resume <thread> -- <prompt>` continues the conversation and names the same thread; the `exec` options go before `resume` (`resume` takes neither `--sandbox` nor `-C`).
- **A real turn** against a dev server of this tree: `bot new prs --model haiku`, `bot say prs "run cornercase status --json…" --wait` ran the command through `Bash(cornercase:*)` with the dev binary first on `PATH`, streamed `thinking`, the `you` line, the step, the `bot` line and `idle`, remembered the conversation on the next message, started a new one after `reset`, stopped a long turn (`stopped`, then `1 held message dropped`), and refused `start --prompt x` and `kill-server` under `CORNERCASE_BOT`. The log held request sizes, never a text.
- Codex's default model of this machine's config was not served by its account, so the Codex runs used a model from its own cache.
- The streams the fixtures in `tests/fixtures/claude/2.1.325/` and `tests/fixtures/codex/0.164.0/` are cut from are those runs.

## Decisions on what the issue left open

- **Claude's brief**: the opening message, for both agents (Rejected above).
- **Context**: both agents compact on their own in print mode; a failure naming the context gets the `bot reset` hint.
- **Codex's own config**: a Codex bot reads the user's, as without cornercase.
- **Unread across restarts**: in memory only.
- **Changing the agent**: `bot edit` refuses another agent; making another bot is the way.
- **Stats**: bot turns do not count.
- **`stop` drops held messages; `reset` and `remove` refuse while a turn runs.**
- **A waited turn counts as shown.**

## Tests

All use `test_util::fake_program` fakes (canned output, recorded argv, folder and environment), so they run on Linux, macOS and Windows (the pane tests of The boundary, `bot_boundary.rs` and the e2e ones, run on Unix only; one pane refusal test, the classifier and the `Lineage` test run on Windows too); no real agent runs. The fake gained `<name>.vars` (names of variables to record in `<name>.env`, a name alone when unset) and `<name>.after` (printed once `<name>.hold` is gone, so a turn can answer only when released). Unit: the chat's appending, healing and replay; the opening message and its caps; the argv of each agent, access and new or resumed session; the session id from both streams; steps; held messages; stop (also of a process tree), timeout and a restart; unread; the 50-bot limit and the name rules; the guard; `PATH` first; and the brief naming only commands that exist. `app::tests::control_requests::bots` drives `bot new` → `say --wait` → `history` → `reset`, the events in order and `status --app`. The e2e tests run the real binary against a fake `claude` first in the config.

The boundary (#856): the classifier of the writing requests (`boundary/tests.rs`); the peer pid of a socket pair; a contained run holding what it starts and nothing else, stopping what it left behind, and (Linux) adopting an orphan; a bot's connection refused its popup, new-pane commands, access changes, `say` to a `full` bot and settings writes, still a bot's after its turn, an unreadable peer taken as a bot's while a turn runs, found by its environment and not when the bot is `full`; and, with real panes (`control_requests/bot_boundary.rs`), every writing request refused into a shell and accepted into an agent, an agent that exited, a held send and the delayed Enter checked again, other clients unchanged; e2e with the real server and socket.

## What later issues add

#846 routines (`queue add --bot`); #847 following the agents a bot starts; #848 chat apps; #849 the other agents. The rest of the boundary (the open list above, #866) and the server-side refusal of `cornercase requests` for a restart or an update (#867) are follow-up issues.

## The window (#845)

The Bots section, the chat and the form are in [bots/window.md](bots/window.md): where the chat shows and what leaves it, the shown bot shared by windows and reported as `shown_bot`, the keys, the drawing of each line, unread and ` go `, the form and `bot edit --about`, why a link is copied and not opened, and what was checked.

## Topics

- [The window](bots/window.md)
