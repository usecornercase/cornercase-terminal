# Bots

Code: `bots/` (`mod.rs`, `brief.rs`, `chat.rs`, `folder.rs`, `turn.rs`), `agents/headless.rs`, `agents/{claude,codex}/headless.rs`, `agents/probe.rs`, `app/bots.rs`, `app/bots/` (`turns.rs`, `manage.rs`), `app/control/bots.rs`, `control/bots.rs`, `cli/bot.rs`, `cli/bot/`, `cli/args/bot.rs`, `cli/help/bot.rs`, `cli/guard.rs`, `paths.rs` (#844)

A **bot** is an agent of your own: a name, a folder, a memory and a chat. It belongs to no project, but it can see all of them and act on them through the `cornercase` command line. It exists for work that belongs to no project, or to all of them ("review my open pull requests every morning", "tell me which issues labelled `bug` came in today, in every project"), which today meant opening a folder as a fake project, or keeping an agent alive in a pane whose context grows for ever.

This is the base of the bots series (#844 to #849). It holds the model, the folders, the conversation, the shared runner, the commands, the requests and the events. Nothing is drawn in the window (#845), there are no routines (#846), no following of the agents a bot starts (#847), no chat apps (#848), and only Claude Code and Codex can be bots (#849 adds the others).

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
- **Time limit: 15 minutes per turn** (`TURN_TIMEOUT`; the app keeps it in `Bots::timeout` so tests shorten it, as Recap does with `RECAP_TIMEOUT`). A bot acts by starting agents and returning; it does not do long work itself. The timeout ends the whole process tree, and the `error` line says `claude did not answer in time`.
- **A restart** drops a turn's process with the server: see Replay above.
- **A second `stop`** adds what was held since to the ones to drop and starts no second job, so a `say --wait` held in between is answered with the dropped notice. **The server stopping** (`stop_background`, so `kill-server`, `restart` and an update) ends every running turn's process tree, one thread each, so the agent does not outlive the server it reports to. A message held while a stop was in flight runs as the next turn: replay keeps `held` at `stopped`, takes off the count a following `N held messages dropped` names, and reads what is left as the turn that runs.
- **`reset` and `remove` refuse while a turn runs** (`stop it first`), the same way: forgetting a session a process still uses, or deleting its folder under it, would leave the chat saying something false.
- **`reset`** (the window's ` new conversation `, #845) forgets the session id and writes `new conversation`. `bot.md` and `memory.md` stay, so the next message starts a new session that reads them.
- **Failures keep the agent's words** (`Not logged in`, `usage limit`). Two are worded by cornercase: `error_max_turns` becomes `claude stopped after 40 steps, its limit for one message`, and a failure whose words name the context (`prompt is too long`, `context window`, `context length`, `maximum context`, `context_length_exceeded`) gets `; start a new conversation: cornercase bot reset <id>`.
- **Unread**: each bot counts the `bot` and `error` lines since a client last showed its chat (`bot seen`). It lives in memory, so a restart shows nothing unread.
- **Notices**: a `bot` or `error` line makes a toast (` <name>: <first line> `, with no ` go `: #845 adds it with the chat) and a desktop notification. A turn that a `say --wait` client waits on counts as shown: it neither notifies nor counts as unread. No client shows a chat until #845, so everything else does. Chat apps are #848.

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
- **`CORNERCASE_BOT=<id>`** (`bots::BOT_ENV`). The command line reads it (see the guard).
- **`PATH`** starts with the folder of the server's own binary (`process::program::own_binary`, `std::env::join_paths`), so `cornercase` is the binary of the server that runs the bot. This matters for a dev build and on Windows (`cornercase.exe`). Codex runs its commands through a login shell (`zsh -lc`), which re-prepends the user's own folders: its `cornercase` may be the installed one rather than this server's, another reason Codex stays `full` only.
- **`CORNERCASE_SOCKET`** is set from `App::shells.pane_socket()` when there is one (a dev build), as Recap's `Ask::socket` is; otherwise the child inherits the server's environment, and a release server's binary finds the default socket. (The issue named `CORNERCASE_SERVER`, but that variable is the boot token that validates `CORNERCASE_PANE`, not the socket.) `CORNERCASE_PANE`, `CORNERCASE_SERVER` and `CORNERCASE_REMOTE` are removed: there is no pane.
- **Account**: as Recap's `Call::home` (the account's folder in the agent's environment variable, and the variables it lists removed), through `App::account_home`, which Recap now shares.

## The shared runner

`agents/headless.rs` is **one runner for Recap and bots** (it was `agents/recap.rs`): `Ask`, `Ran`, `Spec`, `Call`, `run`, with each agent's `headless.rs` holding its arguments and its reader. `Known::headless()` replaces `Known::recap()`; an agent without it can be neither. What bots added on top, none of which changes Recap's arguments (two exact-argv tests, written before the move, pin them):

- `Ask::tools` (`Tools::Recap` or `Tools::Bot(Access)`) picks the arguments; `Ask::conversation` continues a session; `Ask::env` sets (`Some`) or removes (`None`) variables.
- `Ran::session` (the session id out) and `Ran::fresh` (the tools used since the last line, which `run_watched` hands to `Watch::steps` as they arrive). `Action` is a tool and its one-line summary (Bash: the command's first line; Read, Edit and Write: the path; Grep and Glob: the pattern; WebSearch, WebFetch: the query or URL).
- `run_watched(call, ask, Watch)` carries the progress counter, the steps callback and a `probe::Running`. `Spec::bot` says whether the agent can be a bot, with its accesses and why they are limited (`BotSupport`).
- **`probe::Running`** remembers the process of a run and whether it was stopped. `agents::probe::run` used to kill only its direct child on a timeout; with `Running`, a timeout or a stop ends the whole tree through `processes::stop`.

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

**What this is, and is not**: a guard against mistakes, not a boundary. `--setting-sources user` loads your own Claude allow rules, which `--allowedTools` adds to, so a user who allows `Edit` or `Bash(rm:*)` gives every bot that too; and `Bash(cornercase:*)` reaches every pane, so `cornercase send` into a shell pane runs anything that shell can, with no `CORNERCASE_BOT` there. `cornercase` access keeps the agent itself in its folder and to read-only git and gh, and what it drives through cornercase is as powerful as what that reaches. A real boundary (refusing `send`, `keys` and `paste` to panes that run no agent, under `CORNERCASE_BOT`) is a later issue.

Without `CORNERCASE_BOT` (unset or empty) nothing changes. #846 uses the variable as the default for `queue add --bot`, and #847 to tag what a bot starts.

## Limits and log

- A message is at most 20,000 characters.
- **Texts are never logged** (see Log): the log says `bot say bot=prs bytes=120` (and `about_bytes` for `new`), then the turn's agent, model, tokens, duration and how it ended (`answered`, `error`, `stopped`, `timeout`) under the target `bots`. The turn's job returns `Ok` whatever the agent said, so the job never logs an agent's text as its error.
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

All use `test_util::fake_program` fakes (canned output, recorded argv, folder and environment), so they run on Linux, macOS and Windows; no real agent runs. The fake gained `<name>.vars` (names of variables to record in `<name>.env`, a name alone when unset) and `<name>.after` (printed once `<name>.hold` is gone, so a turn can answer only when released). Unit: the chat's appending, healing and replay; the opening message and its caps; the argv of each agent, access and new or resumed session; the session id from both streams; steps; held messages; stop (also of a process tree), timeout and a restart; unread; the 50-bot limit and the name rules; the guard; `PATH` first; and the brief naming only commands that exist. `app::tests::control_requests::bots` drives `bot new` → `say --wait` → `history` → `reset`, the events in order and `status --app`. The e2e tests run the real binary against a fake `claude` first in the config.

## What later issues add

#845 the section, the chat and the form (and ` go ` on the toast); #846 routines (`queue add --bot`); #847 following the agents a bot starts; #848 chat apps; #849 the other agents.
