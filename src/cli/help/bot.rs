pub(in crate::cli) const BOT_ABOUT_HELP: &str = "The bot's id or its name";

pub(in crate::cli) const BOT_AGENTS_HELP: &str =
    "Prints every agent cornercase knows with whether it is installed, whether it can be a bot,
why not (`not yet`, `not installed`) or what limits it (`its sandbox cannot reach cornercase`),
the accesses it supports, its efforts and the models it suggests. A window builds its form from this
list.

With --json it prints {\"agents\": [{\"agent\", \"title\", \"installed\", \"bot\", \"why\", \"accesses\",
\"efforts\", \"models\"}]}.";

pub(in crate::cli) const BOT_NEW_HELP: &str =
    "Makes a bot and prints its id. A bot is an agent of your own: a name, a folder (bots/<id>/ beside
the session: bot.json, bot.md, memory.md and chat.jsonl), a memory and a chat. It belongs to no
project but can see all of them through the `cornercase` command line. Making one starts nothing
and spends no tokens: it only runs when you write to it with `cornercase bot say`.

--access cornercase (the default) holds the agent itself to reading anything, running `cornercase`,
read-only git and gh, and searching the web, and to writing only in its own folder (not bot.json or
chat.jsonl); your own Claude allow rules apply on top. What it does through `cornercase` is as
powerful as what that reaches: it can start agents and type into panes. The commands a bot is
refused and those two files prevent mistakes, not a determined bot. --access full lets it change
any file and run anything, unattended; it is off by default. A Codex bot can have full access only:
its sandbox did not let it reach cornercase on Linux, and macOS and Windows were not tried.

A bot works only while the server runs. With --json it prints the bot as `bot list --json` does.

Example:
  cornercase bot new prs --agent claude --about \"Review my open pull requests and tell me which are blocked\"";

pub(in crate::cli) const BOT_EDIT_HELP: &str =
    "Changes what the bot uses from its next message on; a running answer is not changed. An empty
--model, --effort or --account goes back to the agent's own. The agent cannot change, because
another agent cannot continue the conversation: make another bot instead.";

pub(in crate::cli) const BOT_SAY_HELP: &str =
    "Writes to a bot and starts a turn: the agent runs without its window, continuing the bot's
conversation, and its answer lands in the chat. The command answers at once with the line's id; with
--wait it prints the bot's answer when the turn ends, and exits 3 when the turn fails, is stopped
or times out (15 minutes), with the reason on standard error.

A message sent while the bot answers is held and goes, with the others held, as the next turn. The
first message of a conversation opens with what cornercase tells the bot about itself, then bot.md
and memory.md. A bot runs with CORNERCASE_BOT set, so its `cornercase` commands must name the
project: it runs in no pane.

With --json it prints {\"bot\", \"line\"} (and \"asked\" with --wait): the chat line.

Examples:
  cornercase bot say prs \"which of my PRs are blocked?\" --wait
  echo \"summarize this\" | cornercase bot say prs - --wait";

pub(in crate::cli) const BOT_HISTORY_HELP: &str =
    "Prints a bot's chat as you see it: what you wrote, what the bot used (`▸ cornercase status --json`)
and its answers. --after LINE and --last N cut it.

With --json it prints {\"bot\", \"name\", \"lines\": [{\"id\", \"at\", \"kind\", \"text\"}]}. A kind is you, bot, step,
error or notice; a you line may be held, a step names its tool, and a bot line has its model, tokens
and conversation.";

pub(in crate::cli) const BOT_HELP: &str =
    "A bot is an agent of your own, outside any project: a name, a folder, a memory and a chat. Each
message you write runs one turn of its agent (Claude Code or Codex) in a conversation that continues,
and the answer lands in the chat. It can see every project and act on them through `cornercase`.
Making a bot starts nothing; it spends tokens only when you write to it, and only while the server
runs. Nothing is drawn in the window for it yet.

Events: `cornercase events` reports each chat line, the bot starting or finishing a turn, and a
bot being created, edited or removed. `cornercase status --app` carries the list as `bots`.

Example:
  cornercase bot new prs --about \"Review my open pull requests\"
  cornercase bot say prs \"which of my PRs are blocked?\" --wait
  cornercase bot history prs";
