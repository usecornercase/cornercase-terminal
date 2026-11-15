# Cursor CLI 2026.10.22-27dcf5f fixtures

## Bot turns (Cursor CLI 2026.10.22-27dcf5f)

`bot-new.jsonl`, `bot-resumed.jsonl` and `bot-failed.*` were cut down from real `cursor-agent -p --output-format stream-json --trust -- …` runs of a first turn and one continued with `--resume <id>` (Linux, 2026-11-15, an isolated folder outside every repository, `CORNERCASE*` removed and no cornercase server reachable). Replaced: the session id and the folder (`/work/bots/prs`, `/work/outside`); `thinking` is kept as one delta, and the shell call's parsing details and the files' full contents were cut.. `bot-new.jsonl` keeps three of the checked run's tool calls (a shell command, a write inside, a write of `bot.json` that was refused) and its `result`, whose `result` joins every assistant text of the turn. In `bot-resumed.jsonl` the question and answer are from another run of the same session. `bot-failed.txt` is the line on standard error for an unknown model, cut.

Files under `2026.10.22-27dcf5f/`.
