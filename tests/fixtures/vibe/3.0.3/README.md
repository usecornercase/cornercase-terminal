# Vibe CLI 3.0.3 — discovery

What cornercase needs to integrate Mistral's Vibe CLI (`vibe`, the `mistral-vibe` uv tool), pinned to
**3.0.3** (`vibe --version`, the wheel metadata and the bundled `vibe-rs` binary all report 3.0.3).
The tool auto-updated from 3.0.2 to 3.0.3 while this work was in progress; every shape below was
re-captured on 3.0.3 and the fixtures live in `tests/fixtures/vibe/3.0.3/`.

The wheel's `vibe` is a Python console script that `os.execvpe`s the bundled Rust binary
`vibe/_bin/vibe-rs`; the process keeps its pid. The Rust TUI spawns a Python `vibe-app-server` child.
The Rust TUI speaks **OSC 7501**, and its session store is the "unified" format
(`mistral.vibe.unified-session-store/v1`, minor **10** in 3.0.3).

## How the success and compaction shapes were obtained

The account's Mistral budget is exhausted: `vibe --prompt "Reply with the word done"` exits with
HTTP **402** `billing_vibe_budget_exhausted`, and the same happens in the TUI with the real key.
No real Mistral turn was possible.

The successful turn and the live `/compact` were therefore run through the **real TUI, real unified
store and real OSC code path**, with Vibe's own generic OpenAI-compatible provider pointed at a local
fake server (the same approach the GitHub Copilot integration used). The fake server returns a
streamed completion (`"done"`, `finish_reason: stop`) and usage `input 1234 / cached 1000 / output
12`. Nothing reached Mistral and no account was spent. The failure shape (error then idle) was
captured with the real key and its 402 response.

This is live-proven for the shapes cornercase reads; it is **not** a live Mistral model run.

## What was checked live

- The 3.0.3 launcher/exec topology and the `vibe-app-server` child (as in 3.0.2).
- The trust dialog, its options, the highlight and `trusted_folders.toml` (`trust.txt`).
- The OSC 7501 support query, idle, working, **done** and error reports (`osc.txt`).
- A successful turn's store: `meta.json`, `CURRENT` (minor 10), `projection-state.json`,
  `checkpoint.json`, `runtime-state.json` and the journal (`success/`).
- The TUI footer after a successful turn: `probe[off] · 1.25k/140k tokens (1%)`
  (`success/screen.txt`).
- A live `/compact`: `✓ Compaction completed.`, the footer at `0/140k`, `compaction_count` 1 and
  `last_reported_context_tokens` null (`compact/`).
- The failed-turn real-key shape: `state=error` then `state=idle`.

Read from the shipped 3.0.3 source, not run live: the subagent lifecycle mapping
(`runtime-state.json` `subagents.children`, `reserved`/`running` count, `tombstone`/`deleting_*`
close), `--resume` semantics, provider/model internals and the compaction request's contents.

## OSC 7501

The TUI asks once with `ESC ]7501;?` and only reports after the terminal's support reply (cornercase
answers it). The successful run:

```
state=idle:app=vibe
state=working:app=vibe:msg=VmliZQ==
state=done:app=vibe:msg=VmliZQ==
```

`done` is real: cornercase gets the unseen-finish mark (`✓` and the finish notice) from Vibe's own
completion report, not from inferring idle. `msg` is base64 ("Vibe"); the working report carries the
same message. A failed turn reports `error` and then `idle` at once, so the red `!` can pass before
the one-second failure notice; the store's `latestTurn.status` still says `failed`.

`/compact` emits **no** OSC 7501 report. The TUI prints `✓ Compaction completed.` and the store's
`compaction_count` grows; cornercase confirms the compact from that count, as it does for other
agents.

The terminal title is OSC 0 `Vibe` and carries no state.

## Session store (minor 10)

```
active/<session id>.lock.json   {"acquired_at","lease_version","process_id","session_id"}
unified/<id>/CURRENT            {"generation","manifest_sha256","session_id","snapshot_sequence",
                                 "store_format":"mistral.vibe.unified-session-store/v1","store_format_minor":10}
unified/<id>/meta.json          {"session_id","environment":{"working_directory"},"config":{"active_model","model_name"},...}
unified/<id>/journal/NNNN.jsonl records {"sequence","type","payload","record_sha256","previous_record_sha256"}
unified/<id>/generations/<g>/   checkpoint.json, manifest.json, projection-state.json, runtime-state.json
```

3.0.3 changed the context window's place. The old minor-9 store (3.0.2) had
`core_settings.context.compaction.token_threshold`; minor 10 has
`core_settings.context.model_facts.compaction_threshold` (and `compaction_threshold_max`,
`context_window`). The TUI footer's denominator is the **compaction threshold**:
`model_facts.compaction_threshold` 140000 in `success/`, which is why the footer says `140k`.
Cornercase reads `model_facts.compaction_threshold` first and falls back to the old
`compaction.token_threshold`, and shows no percentage when neither is present (never a guessed
window).

`projection-state.json` (success):

```json
{"latestTurn":{"id":"turn-...","status":"completed","startedAt":...,"completedAt":...},
 "session":{"contextUsage":{"inputTokens":1234,"cachedInputTokens":1000,"outputTokens":12,"totalTokens":1246},
            "tokenUsage":{"inputTokens":1234,"cachedInputTokens":1000,"outputTokens":12,"totalTokens":1246},
            "status":{"type":"idle"}}}
```

After `/compact` (`compact/projection-state.json`): `contextUsage` is all zeros, `tokenUsage` stays
cumulative (2468 input, 2000 cached, 24 output in the captured session), `latestTurn` is a new
completed turn. `compact/checkpoint.json` has `compaction_count: 1` and `last_reported_context_tokens:
null`; the completed turn has `turn.outcome.output: [{"text":"done","type":"text"}]`.

The percentage cornercase shows is the TUI's own figure: `(inputTokens + outputTokens) /
compaction_threshold`, rounded and capped. The model is `session_metadata.active_model` (falling back
to `meta.json.config.active_model`).

## Stats

- Prompts: each `command_reserved` `turn/start` in the journal, at `latestTurn.startedAt`. The
  compaction command is `app_server/session/compact` and is not a prompt.
- Tokens: the session's cumulative `tokenUsage`, split into `input = inputTokens - cachedInputTokens`,
  `cache_read = cachedInputTokens`, `out = outputTokens`. `tokenUsage` has no cache-write field, so
  cache writes are left out (the per-call `action_result` usage in the journal does carry
  `cache_write_tokens`; stats do not read the journal for tokens). Compaction model calls are included
  in the cumulative total but are not prompts or turns. The first read only sets the baseline, so an
  old session is not imported into Stats.
- Turns and time come from the OSC 7501 samples like every agent (working/idle), not from the store.

## Journal shapes

`success/journal.jsonl` is the turn/start and the model reply; the full journal also has `core_input`
(the system prompt and tools, large) and `projection_delta` records, which the fixture leaves out.
The reply:

```json
{"type":"action_result","payload":{"result":{"result":{
 "finish_reason":"stop","parts":[{"text":"done","type":"text"}],
 "usage":{"cache_write_tokens":0,"cached_input_tokens":1000,"input_tokens":1234,
          "output_tokens":12,"reasoning_tokens":0,"total_tokens":1246}}}}}
```

`last_message` reads the newest `parts[]` entry of type `text`; search reads the text blocks of
`turn/start`'s `params.message` and those text parts. Records carry no timestamps, so `written` is `None`.

`compact/journal.jsonl` holds the `app_server/session/compact` command (`params.instructions: ""`)
and the first reply. The compaction's own model call is not an `action_result`; its tokens appear only
in the cumulative `tokenUsage`.

## Trust

`trust.txt` is the screen. The dialog says `Trust this folder?`, detects `AGENTS.md` (or `.vibe/`,
skills), offers `1. Trust folder` (highlighted) and `2. Don't trust`, and Enter picks the folder.
`DEFAULT_TRUST_PROMPT` already matches, so no pattern or `answer_keys` change. The answer is saved in
`<VIBE_HOME>/trusted_folders.toml`; `--trust` grants it for one programmatic run. Headless mode never
prompts: it warns that project configuration is ignored and continues.

## Process, flags, prompts and resume

```
vibe (Python console script) --execvpe--> vibe-rs (Rust TUI, same pid) --> python vibe-app-server
```

The foreground process is `vibe-rs`, so `vibe::runs_as` takes `vibe-rs` (and `detect` also looks at
`argv[0]`). The app-server's pid is the one the lease names.

`--agent ask|accept-edits|smart-approve|auto-approve` (default `accept-edits`; no plan mode);
`--auto-approve` (`--yolo`) and `--smart-approve`; `--add-dir`; `--workdir`; never `--worktree`.
`--yolo`, `--auto-approve`, `--smart-approve` and `--agent=<name>` normalize to `--agent <name>`.
No `--model` flag (`active_model`, `/model`, `VIBE_ACTIVE_MODEL`); no effort flag (`/thinking`).
`/compact` takes optional instructions. Prompts typed while a turn runs are queued and merged
(`HOLDS_PROMPTS`); `!`, `/` (except `/skill`) and `&` need idle, so they are local for confirmation.
`--resume <id>` takes the session folder and is not folder-scoped; `-c`/bare `--resume` are. Session
logging must stay on for the id to exist.

## Constants (implemented in `src/agents/vibe/`)

| constant | value | note |
| --- | --- | --- |
| `NAME` / `TITLE` / `COMMAND` | `vibe` / `Vibe CLI` / `vibe` | |
| `MODEL_FLAG` / `MODELS` | `None` / `&[]` | no `--model`; a model is refused, not passed |
| `UNATTENDED` | `["--agent", "auto-approve"]` | fails loudly when an admin forbids it |
| `MODES` | ask, smart approve, auto-approve (dangerous) | flags normalize to `--agent …` |
| `EFFORT` | `None` | `/thinking` only |
| `COMPACT` | `/compact`, keeps | optional instructions |
| `ADD_DIR` | `--add-dir` | repeatable |
| `HOLDS_PROMPTS` | `true` | queued and merged |
| `LOCAL_COMMAND` | `/`, `!`, `&`, except `/skill` | they need idle |
| `INPUT_BOX` / `SURVEY` | `None` / `false` | no marker observed |
| `STATUS_BY_FOLDER` / `RECORD_BY_FOLDER` | `false` / `false` | the lease's pid decides |
| `resume` | `args + ["--resume", id]` | id = session folder |
| `ACCOUNT` / `USAGE` | `None` | no per-account/quota source yet |
| `UNFOUND` | `Vibe CLI has no conversation for this pane yet` | |

## Fixtures

- `success/` — a completed turn: `CURRENT`, `meta.json`, `projection-state.json`, `checkpoint.json`,
  `runtime-state.json` (trimmed to the fields cornercase reads), `journal.jsonl` (turn/start + reply),
  `screen.txt` (the TUI footer).
- `compact/` — the same session after `/compact`: zeros in `contextUsage`, `compaction_count` 1,
  `last_reported_context_tokens` null, the compact command, and the screen with
  `✓ Compaction completed.` and `0/140k`.
- `osc.txt`, `trust.txt` — the OSC exchange and the trust screen.

Paths and the username were replaced with `/work` and `user`; the session ids, timestamps and usage
numbers are the captured ones. The fixtures are samples for the readers and tests, not replayable
journal chains (the records are a subset and their hashes are left as captured).

## Known limits

- No live Mistral run was possible (402 budget exhausted); the success/compaction shapes come from the
  local fake provider driving the real TUI.
- A failed turn reports `error` then `idle` at once, so its red `!` may not hold long enough to notify;
  waiting approvals (`blocked:kind=permission|question`) persist and notify normally.
- `send --enter` after a prompt queued behind a running turn is confirmed only when its turn starts
  (`latestTurn.startedAt`), which can lag.
- `tokenUsage` has no cache-write field; stats leave cache writes out. A multi-call turn is counted
  through the cumulative total, so all calls are included, but the per-call split is not.
- Subagents: the reader counts `subagents.children` with `reserved`/`running`; a live running child
  was not captured on 3.0.3.
