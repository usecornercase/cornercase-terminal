# opencode fixtures

These fixtures target **opencode 1.18.35** (`opencode --version`, the npm package `opencode-ai`), checked locally in a scratch `XDG_DATA_HOME`. The message rows are the `data` column of real rows from that version's database, with paths replaced by `/tmp/project` and ids by placeholders; the part rows keep the shape of real ones with their text, snapshots and commands replaced. No fixture holds a conversation or a credential.

- `schema.sql`: the `project`, `session`, `message` and `part` tables and their indexes as 1.18.35 creates them (38 migrations, the last `20260622170816_reset_v2_session_state`), in WAL mode. opencode keeps one database for every process, `$XDG_DATA_HOME/opencode/opencode.db` (`opencode-<channel>.db` outside the latest, beta and prod channels, or `OPENCODE_DB`), and the TUI holds it open while it runs. No table holds a pid.
- `reply.jsonl`: a user message and the assistant reply that finished it (`time.completed`, `finish: "stop"`). Its tokens add up to 124,878, 12 % of DeepSeek V4 Pro's 1,000,000-token window.
- `working.jsonl`: a turn in progress: the assistant message has no `time.completed` yet, and no tokens.
- `aborted.jsonl`: a turn interrupted with Esc: the assistant message is completed, with `error.name` `MessageAbortedError` and no tokens.
- `compaction.jsonl`: `/compact`: a user message, then an assistant reply with `mode: "compaction"` and `summary: true`. Its text part holds the summary, which `read --last-message` leaves out. `session.time_compacting` stayed empty throughout.
- `reply-parts.jsonl`: the parts of `reply.jsonl`'s assistant message, in id order: `step-start`, `reasoning`, the `text` it answered with (`time.start` and `time.end`) and `step-finish`. A message's text lives only in its parts; one turn is an assistant message per step, and only the steps that speak have a `text` part. opencode marks text it writes for the model alone (a plan-mode reminder, the prompt to carry on after a compaction) `synthetic: true`; `read --last-message` leaves it out.
- `tool-parts.jsonl`: a step that only ran a tool: `step-start`, a completed `tool` part and `step-finish`, no text.
- `models.json`: the DeepSeek entries of the models.dev list opencode caches in `$XDG_CACHE_HOME/opencode/models.json` (5 MB for every provider), where names and `limit.context` come from.

opencode's footer shows `<tokens> (<percent>)` from the newest assistant message with output: `input + output + reasoning + cache.read + cache.write` over the model's `limit.context`, rounded. cornercase shows the same. A pending permission or question is never written: the tool part already reads `running` while opencode asks, and its terminal title stays `OpenCode` or `OC | <session title>` whatever it does, so an opencode tab never shows `!`.

## Bot turns (opencode 1.18.35)

`bot-new.jsonl`, `bot-resumed.jsonl` and `bot-failed.*` were cut down from real `opencode run --format json -- …` runs of a first turn and one continued with `--session <id>` (Linux, 2026-11-15, an isolated folder outside every repository, `CORNERCASE*` removed and no cornercase server reachable). Replaced: the session id and the folders (`/work/bots/prs` and `/work/outside`); the user's `external_directory` rules in the permission error were cut.. `bot-new.jsonl` keeps four of the seven steps of the checked run (a bash command, a write inside, a write outside that was refused, a write of `bot.json` that was refused) and its final text. `bot-failed.jsonl` is a run with an unknown model.

Files under `1.18.35/`.
