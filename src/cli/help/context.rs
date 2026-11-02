pub(in crate::cli) const CONTEXT_HELP: &str =
    "Prints what fills an agent's context, as the Context dialog shows it (`view context` in a tab's
menu or an agent row's): how its window splits between the system prompt, the tools, MCP servers,
memory files (AGENTS.md, CLAUDE.md), skills and the conversation, with each memory file, MCP server
and skill.

For Claude Code it starts a second `claude` that reloads the same conversation, in the same
folder, account and model, sends no prompt and spends no tokens. It turns the hooks off and waits
up to 8 s for the MCP servers to connect, so a read takes 3 to 10 s. The numbers are a reload of
the conversation with today's settings: while the agent works they lag by the reply in progress.
A pane with no conversation yet answers with what a new one starts with. Without --pane or --tab
it reads the pane it runs in. A pane that runs no agent exits with 1. The other agents answer
their model and how full the window is, with a notice that they do not say what fills it.

--cached prints the last answer with its age and starts nothing, so a script can read it cheaply.

A read that fails says why on the line `context unavailable:` (with --json, in `error`, beside the
numbers of the last good read) and the command exits with 3.

With --json it prints {\"pane\", \"agent\", \"title\", \"model\", \"percent\", \"compacted\", \"loading\",
\"read_at\", \"age\", \"breakdown\", \"notice\", \"error\"}. breakdown is null until a read worked, else
{\"model\", \"conversation\", \"total_tokens\", \"max_tokens\", \"percent\", \"auto_compact_at\",
\"categories\" (each {id, name, tokens, kind}, the kind being used, deferred, buffer or free),
\"memory_files\" ({path, kind, tokens}), \"mcp_servers\" ({name, tools, loaded, tokens}),
\"mcp_instructions\" ({name, tokens}), \"skills\" ({total, included, tokens}), \"messages\"}. The
messages are tokens too: the calls, results, attachments, assistant and user text, and per tool
and attachment kind.

Examples:
  cornercase context
  cornercase context --pane 4 --json
  cornercase context --tab 3 --cached";
