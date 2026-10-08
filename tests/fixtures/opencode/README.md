# opencode fixtures

These fixtures target **opencode 1.18.35** (`opencode --version`, the npm package `opencode-ai`), checked locally in a scratch `XDG_DATA_HOME`. The message rows are the `data` column of real rows from that version's database, with paths replaced by `/tmp/project`, ids by placeholders and text parts left out (they live in the `part` table, which cornercase never reads). No fixture holds a conversation or a credential.

- `schema.sql`: the `project`, `session` and `message` tables and their indexes as 1.18.35 creates them (38 migrations, the last `20260622170816_reset_v2_session_state`), in WAL mode. opencode keeps one database for every process, `$XDG_DATA_HOME/opencode/opencode.db` (`opencode-<channel>.db` outside the latest, beta and prod channels, or `OPENCODE_DB`), and the TUI holds it open while it runs. No table holds a pid.
- `reply.jsonl`: a user message and the assistant reply that finished it (`time.completed`, `finish: "stop"`). Its tokens add up to 124,878, 12 % of DeepSeek V4 Pro's 1,000,000-token window.
- `working.jsonl`: a turn in progress: the assistant message has no `time.completed` yet, and no tokens.
- `aborted.jsonl`: a turn interrupted with Esc: the assistant message is completed, with `error.name` `MessageAbortedError` and no tokens.
- `compaction.jsonl`: `/compact`: a user message, then an assistant reply with `mode: "compaction"` and `summary: true`. `session.time_compacting` stayed empty throughout.
- `models.json`: the DeepSeek entries of the models.dev list opencode caches in `$XDG_CACHE_HOME/opencode/models.json` (5 MB for every provider), where names and `limit.context` come from.

opencode's footer shows `<tokens> (<percent>)` from the newest assistant message with output: `input + output + reasoning + cache.read + cache.write` over the model's `limit.context`, rounded. cornercase shows the same. A pending permission or question is never written: the tool part already reads `running` while opencode asks, and its terminal title stays `OpenCode` or `OC | <session title>` whatever it does, so an opencode tab never shows `!`.
