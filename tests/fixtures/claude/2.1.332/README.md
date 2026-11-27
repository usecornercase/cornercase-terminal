# Claude Code 2.1.332 fixtures

Claude Code 2.1.332, macOS, 2026-11-27.

Transcripts of a turn that calls a tool, cut down from real ones to the lines `read --pending` looks at and their envelope (`parentUuid`, `isSidechain`, `type`, `uuid`, `timestamp`, `sessionId`, `version`, `message.{model,id,role,content,stop_reason}`), with every text, id, path and name replaced. Each tool call is an `assistant` line of its own; the `hook_success` attachment that follows names the call's id and is not its result; only a `user` line's `tool_result` block is.

- `ask-one.jsonl`, `ask-several.jsonl`, `ask-multi-select.jsonl`: an `AskUserQuestion` still waiting for its answer, with one question, two (an option without a description among them), and one that takes several options.
- `ask-answered.jsonl`: the same question answered (`permissionDecision`, `toolUseResult` with the answers).
- `ask-rejected.jsonl`: the same question rejected (`is_error`, `toolDenialKind: user-rejected`).
- `permission-bash.jsonl`: a `Bash` call with no result yet; the transcript reads the same whether it waits for a permission or runs.
- `parallel.jsonl`: two calls of one answer (`Read`, then `Bash`, the same `message.id`), with the `Read`'s result between them, so only the `Bash` is left.
