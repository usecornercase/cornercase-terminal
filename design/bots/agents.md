# Bots: the agents (#849)

Which agents can be a bot, with which access, and what was checked live to decide it. The model, the access and the runner are in [Bots](../bots.md); this is the table and the reasons per agent.

An agent can be a bot when its command line has all three of these:

1. **A non-interactive mode with machine-readable output**: the final answer, its tool uses (for the `step` lines) and, when it gives them, its tokens.
2. **A session id in that output and a way to continue it** in a later run.
3. **Permissions fixed up front**: rules or a sandbox that hold the `cornercase` access (read, run `cornercase`, write only in the bot's folder), and a bypass for `full`.

Each agent's `headless.rs` is its spec in the shared runner (`agents/headless.rs`). An agent with the first two and without the third is `full` only, with the reason in `BotSupport::why`.

## What each has (checked on Linux, Ubuntu, 2026-11-15)

| Agent (version) | Headless + JSON | Session + resume | Permissions | Result |
|---|---|---|---|---|
| Claude Code 2.1.325 | yes | yes | yes | `cornercase` and `full` (#844) |
| Codex 0.164.0 | yes | yes | sandbox cannot reach the socket | `full` only (#844) |
| Gemini CLI 0.67.0 | yes (`--output-format stream-json`) | yes (`init.session_id`, `--resume`) | yes (a `--policy` file, `--approval-mode yolo`) | `cornercase` and `full`; `full` only on Windows |
| opencode 1.18.35 | yes (`run --format json`) | yes (`sessionID`, `--session`) | no: a rule lets a redirect write outside | `full` only |
| Cursor CLI 2026.10.22-27dcf5f | yes (`-p --output-format stream-json`) | yes (`session_id`, `--resume`) | no: its edit tool cannot be kept in a folder | `full` only |
| GitHub Copilot CLI 1.0.98 | yes (`--output-format json`) | yes (`result.sessionId`, `--resume=`) | yes (`--allow-tool`, `--deny-tool`) | `cornercase` and `full`; `full` only on Windows |
| Grok Build 1.0.70 | not run: not signed in | not run | not run | `not yet` |
| Vibe CLI | not installed, not checked | | | `not yet` |

None was logged in or installed for this: an agent that was not signed in (Grok) or not installed (Vibe) stays `not yet` instead of claiming something nobody checked. The issue's sample reason, `no way to continue a conversation without its interface`, would have been a claim, so `why` says `not yet`. The follow-up issue (#879) lists them.

Each run was made in a fresh folder outside every repository, with `CORNERCASE*` and the Claude Code variables removed and `CORNERCASE_SOCKET` pointing at a socket that does not exist, so no agent reached the real server. The fixtures of `tests/fixtures/<agent>/<version>/bot-*.jsonl` are cut from those runs (the README of each says what was replaced).

## Gemini CLI

- **Arguments.** `gemini --skip-trust --output-format stream-json --approval-mode default --policy <folder>/.gemini/cornercase-policy.toml [--model M] [--resume S] --prompt=<prompt>`; `full` is `--approval-mode yolo` without `--policy`. Without `--skip-trust` it exits 55 in an untrusted folder, and the bot's folder is not one it knows. `--prompt=<text>` as one word keeps a prompt that starts with `-`. There is no effort switch.
- **The stream.** `init` (session, model), `message` (the user's and the assistant's, which arrives in pieces with `delta`), `tool_use` and `tool_result`, then `result` with `stats` (`input` is the uncached input, `cached`, `output_tokens`, and `models` with each model's tokens: the router model and the main one appear, so the model shown is the one with the most tokens). An error is a `result` with `status: error` and its message; Gemini's standard error always starts with a colour warning, so it is not read.
- **The `cornercase` access is a policy file**, not the deprecated `--allowed-tools` (it warns that it goes in 1.0 and cannot deny a file). `Spec::prepare` writes `<folder>/.gemini/cornercase-policy.toml` before every turn, from the runner's own lists (`GH_READS`, `Ask::stops`, `Ask::protected`), so a file that was changed never outlasts one turn. It allows `cornercase`, the read-only gh prefixes, and writes with `write_file` and `replace`; it denies the commands that stop the server, and writes of `bot.json`, `chat.jsonl`, `secrets`, `.gemini/` and `.env` (Gemini reads a trusted folder's `.env` into its own environment, which `GIT_EXTERNAL_DIFF` could turn into a shell; not checked live). Gemini tests a rule's `argsPattern` against the arguments as JSON, but writes to the path it decodes and resolves afterwards, so a name could slip past by `%2E`, by a `"` that ends the match (`<folder>/"/../.gemini/x`) or, on macOS, by case. So the name rules are case-insensitive and a third rule denies any write whose path holds a `%` or a backslash. The name rule is a case-insensitive *substring* match, so `secrets` (#877) also keeps a Gemini bot from writing any path that holds that word, a safe narrowing; the rule for `secrets` was not run against Gemini. The rules were checked live before these two additions and are pinned by a test over the `regex` crate; the additions were not re-run against Gemini. Checked live: a write outside the folder is refused by Gemini's own workspace rule, `touch`, `cornercase … > outside`, `&&`, `;`, `$(…)`, backticks, `| tee` and `>>` are refused, `cornercase --version` and a write of `notes.md` run, a deny beats the allow, and a resumed turn keeps the session.
- **Why `.gemini/` is denied.** `--skip-trust` makes the bot's folder trusted, so a `.gemini/settings.json` there would be read as the project's settings on the next turn, and it can carry allowed tools, hooks or MCP servers: a bot that can write in its folder could give itself any shell. It allows no git since #893: the bare git prefixes gave a bot nothing (its folder is not a repository) and opened the made-repository hole of The boundary › A project's git, which was checked for Claude only. A project's git for Gemini needs a live check of `commandPrefix` per folder plus a `commandRegex = "git .*--output"` deny, per 0.67.0's own `docs/reference/policy-engine.md`; Gemini 0.67.0 was not signed in on the machine of #893 (#900).
- **It reads only its folder.** `read_file /etc/hostname` is refused (`Path not in workspace`); `web_fetch` works. This is narrower than the access promises, and a safe narrowing: the brief says `read and write only in your folder` for these agents (`BotSupport::reads_anywhere`), `bot agents` says it as `reads: "folder"` (feature `bot-reads`) so a form can write the same in its note under Access, and cornercase's own commands still show the user's work.
- **A read-only mode exists** (`--approval-mode default` without a policy: `read_file` works, `write_file` and `run_shell_command` come back as not found). It is the lead for Recap on Gemini, see below. One trivial run took 214 s.
- **Windows is `full` only** with `its rules are unchecked on this system`: the checks were seen with `sh`, and PowerShell evaluates `(…)` and `$(…)` inside a command's arguments.

## opencode

- **Arguments.** `opencode run --format json [--auto] [--session S] [--model M] -- <prompt>`; `--auto` is the bypass and goes only with `full`.
- **The stream.** Every line has `type`, `sessionID` and `part`: `step_start`, `text`, `tool_use` (one per tool, sent when it ends, `state.input` holding the command or path) and `step_finish` (`part.reason` is `tool-calls` or `stop`, with `tokens` of that step, which are added). The turn ends at the `step_finish` that is not `tool-calls`. There is no model in the stream. An error is an `error` line.
- **Why `full` only.** `OPENCODE_PERMISSION` with `bash: {"cornercase *": "allow"}`, `edit` denied for `bot.json` and `chat.jsonl` and `external_directory: deny` held for a write outside, `touch`, `&&` and `bot.json`, but **`cornercase --version > <outside>/file` ran and wrote the file**: a rule on a command cannot keep its output in the folder. The reason is `a command can write outside its folder`.

## Cursor CLI

- **Arguments.** `cursor-agent -p --output-format stream-json --trust [--force] [--resume S] [--model M] -- <prompt>`; `--trust` is always there (it refuses an untrusted folder otherwise) and `--force` is the bypass.
- **The stream.** `system/init` (session, model, which is a display name), `user`, `thinking` (ignored), `assistant` (text), `tool_call` `started` and `completed` (the key is `shellToolCall`, `editToolCall` and so on) and `result` with `usage` in camelCase, where `inputTokens` leaves out cache reads. **`result.result` joins every assistant text of the turn**, so the answer is the text after the last tool call, and `result` is only the fallback when there is none. An error is on standard error only, as one line.
- **Why `full` only.** By default `-p` rejects shell commands that are not allowed, but **its edit tool wrote outside the folder and wrote `bot.json`**. With a workspace `.cursor/cli.json` (allow `Shell(cornercase)` and `Write(<folder>/**)`, deny `Write(<folder>/bot.json)`) it refused `bot.json`, `touch` and `&&`, but **a write outside still worked and `cornercase … > outside` ran**: an allow does not narrow the edit tool and a deny wins over an allow, so "only inside the folder" cannot be said. `--sandbox enabled` did not start on this machine (AppArmor). The reason is `its edits can leave its folder`.

## GitHub Copilot CLI

- **Arguments, `cornercase`.** `copilot --output-format json --no-auto-update --no-ask-user --disallow-temp-dir --disable-builtin-mcps --allow-all-urls --allow-tool shell(cornercase:*)`, then `--allow-tool shell(<c>:*)` for each read-only gh command, `--allow-tool write`, `--deny-tool shell(cornercase <stop>:*)` for each command that stops the server, `--deny-tool write(<folder>/<file>)` for each protected file, then `[--resume=S] [--model M] [--reasoning-effort E] --prompt=<prompt>`. Each `--allow-tool X` is two words, as checked. `full` is `--allow-all --no-ask-user`.
- **The stream.** Many event types; these matter: `session.auto_mode_resolved` (model), `assistant.message` (`content`, `model`, `parentToolCallId` when a subagent wrote it, `chunkIndex` when the answer is split), `tool.execution_start` (`bash` and `powershell` with a command, `apply_patch` with a patch string, other tools with a path or URL) and `result` (`sessionId`, `exitCode`). A new message from the main agent replaces the text, a chunk after the first adds to it, so the answer is what follows the last tool.
- **Checked live.** A write outside, `touch`, `> outside`, `&& touch`, `| tee`, `sed -i` and `find -delete` outside, `cat /etc/hostname`, `view /etc/hostname` and a write of `bot.json` are refused; `$(…)` was blocked before it ran; prefix rules are exact (`gh pr list` ran, `gh pr create` did not); the deny of `cornercase restart` beats the allow; a resume with the same arguments keeps the session. **A Copilot bot reads only its folder**, as Gemini does. Windows is `full` only for the same reason as Gemini.
- **No git (#893, 2026-11-16, Copilot 1.0.98 through `npx`).** With `shell(git log:*)` allowed, `git -C <a project> log` was still refused, and so were `-c`, `--output` and `--no-index` with folders outside the bot's. Its rules work per first-level subcommand, so they cannot name a folder or deny `--output`, and its path check keeps file access in the working folder (`--add-dir` or `--allow-all-paths` would let `write` leave the bot's folder). Whether the refusal came from the stem rule or the path check was not told apart.
- **Tokens are zero.** The stream gives only the session's running total (`session.usage_checkpoint`), which on a resumed turn is the sum of both, so there is no per-turn count to show.
- **Effort** goes only with a named model that is not `auto`, so `bot new` and `bot edit` (and the form) refuse an effort without one (`BotSupport::effort_needs_model`), instead of accepting a choice that does nothing (`auto` fails with `Model "auto" does not support reasoning effort configuration`). This account offers no model by name, so an effort with one was **not checked**.
- **The environment.** `COPILOT_ALLOW_ALL` (`true` would allow every tool) and `COPILOT_ASSISTED_APPROVAL` are removed with Copilot's session variables. Not run live: the variables were not set here.

## Standard error (`Spec::stderr`)

Cursor and Copilot say why they stopped only on standard error, as one line (`Error: Model "x" from --model flag is not available.`, `Cannot use this model: …`), with no `result`. Their specs ask for it: `probe::Limits::stderr` pipes it, a thread keeps the first non-empty line (cut to 200 characters) and reads on so the pipe never fills, and a run that ends without an answer says `<agent> exited without answering: <that line>`. The other agents keep it discarded: Gemini's always starts with a colour warning and its errors are in its stream.

## The answer

The four agents speak before and between tools. The answer is only what the agent says after its last tool use (`headless::heard` counts a step and clears the text), as Claude's and Codex's already are.

## The runner refuses what the spec does not list

Before any process starts, `run_watched` refuses a Recap from an agent whose spec cannot write one (`<agent> cannot write a recap`) and an access a spec does not list (`<agent> cannot be a bot with <access> access`). So "full only" holds also for a hand-edited `bot.json`, not only in the form.

## Windows

A headless prompt has line breaks (the brief). Rust refuses CR and LF in a batch file's arguments and `cmd.exe` would cut the line anyway, so an agent installed with npm (Gemini on Windows only comes that way, and so can Codex) could not start. `process::program::command_without_cmd` starts what an npm shim starts directly, as launches do (#632). **Not run on Windows.**

## Recap (#849)

**Recap stays with Claude Code and Codex**: `Spec::recap` says which agents can write one, and the runner refuses the others. The reasons are in [Recap › Which agents write a recap](../recap.md#which-agents-write-a-recap-849); Gemini CLI's read-only mode (see above) is the lead for a follow-up.

## Not checked

macOS and Windows for all four; Grok (not signed in) and Vibe (not installed); Copilot's effort with a named model and its environment variables; Cursor's sandbox on macOS; opencode and Cursor after a newer version; opencode in a long resumed session, where it may compact on its own first (its summary is an assistant message that finishes with `stop`, which the reader could take for the answer: the stream was not seen with a compaction in it).
