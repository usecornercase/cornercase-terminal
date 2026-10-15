# Cursor CLI 2026.10.14-941492e

Nothing here was captured from a live session: running one spends a Cursor account's requests, and the
store of a chat that never got a turn holds nothing. The layout below was read from the bundle's code
(`~/.local/share/cursor-agent/versions/2026.10.14-941492e/`), and `agents::cursor::fake::FakeCursor` builds
synthetic stores that follow it, so the tests pin what cornercase expects, not what Cursor writes.

- `store.db` (SQLite): `blobs(id, data)`, the id the lowercase hex SHA-256 of the data, and `meta(key, value)`
  with one row `0` holding the hex of a JSON with `latestRootBlobId`, `lastUsedModel` and `createdAt`.
- Root blob, protobuf `agent.v1.ConversationStateStructure`: field 5 `token_details` (`1 used_tokens`,
  `2 max_tokens`), field 8 the turn blob ids.
- Turn blob: field 1 an agent turn (`1` user message blob, `2` step blobs, `7` routed model display name,
  `10` user message id) or field 2 a shell turn. User message: `1` text, `2` message id, `25 started_at_ms`.
  Step: `1` assistant message (`1` text, `2` started, `3` completed), `2` tool call, `3` thinking.
- Transcript, `projects/<slug>/agent-transcripts/<chat id>/<chat id>.jsonl`: `{"role":"user"|"assistant",…}`
  lines and `{"type":"turn_ended","status":"success"|"aborted"|"error"}`; no timestamps.
- Title with `display.showStatusIndicators`: `<name> - ⏳ Working ···`, `🧭 Planning`, `⌨️ Running shell command`,
  `📋 Queued`, `📝 Reviewing changes`, `❓ Waiting for you`, `🔐 Waiting for confirmation`, `✅ Ready`.

To check against a real Cursor: run it in a scratch git folder, send a prompt, and look at
`/proc/<pid>/fd` (does it hold `chats/<md5>/<id>/store.db`?), whether `started_at_ms` is filled, and what
`token_details` holds after a reply, a `/summarize` and a `/model` switch. If the process does not hold
the store, the reader needs the folder rule (`RECORD_BY_FOLDER`).
