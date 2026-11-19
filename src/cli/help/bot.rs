pub(in crate::cli) const BOT_ID_HELP: &str = "The bot's id or its name";

pub(in crate::cli) const BOT_AGENTS_HELP: &str =
    "Prints every agent cornercase knows with whether it is installed, whether it can be a bot,
why not (`not yet`, `not installed`) or what limits it (`its sandbox cannot reach cornercase`,
`a command can write outside its folder`),
the accesses it supports, `reads` (`folder` when its `cornercase` access reads only its own folder, left
out otherwise), its efforts and the models it suggests. A window builds its form from this
list.

With --json it prints {\"agents\": [{\"agent\", \"title\", \"installed\", \"bot\", \"why\", \"accesses\",
\"reads\", \"efforts\", \"models\"}]}.";

pub(in crate::cli) const BOT_NEW_HELP: &str =
    "Makes a bot and prints its id. A bot is an agent of your own: a name, a folder (bots/<id>/ beside
the session: bot.json, bot.md, memory.md and chat.jsonl), a memory and a chat. It belongs to no
project but can see all of them through the `cornercase` command line. With --about, making it
starts its first turn at once: the bot reads what it is for, schedules what that asks for and says in
its chat what it will do (`cornercase bot setup` runs it again). Without --about it starts nothing
and spends no tokens until you write to it with `cornercase bot say`.

--access cornercase (the default) holds the agent itself to reading anything, running `cornercase`,
read-only git in your projects (`git -C <folder> log`, `show`, `diff`, `status`, never `--output`) and
gh, and searching the web, and to writing only in its own folder (not bot.json, chat.jsonl or a name starting with - at its top); your
own allow rules apply on top. The server refuses what its `cornercase` commands
type into a pane that runs no agent, a clone, and stopping, restarting or updating the server; it
can still start agents and write to them. Your own allow rules, an agent's own shell mode and
permission prompts, flows, plugins, closing or stopping things and --remote are not
a boundary: give it work you would give an assistant at your keyboard. --access full lets it change
any file and run anything, unattended; it is off by default. A Codex, opencode or Cursor CLI bot can
have full access only: Codex's sandbox did not let it reach cornercase on Linux, and macOS and Windows
were not tried; opencode's and Cursor CLI's rules cannot keep its writes in its folder. A Gemini CLI or
GitHub Copilot CLI bot with cornercase access reads only its own folder and runs no git; on Windows they support
full access only.

A bot works only while the server runs. With --follow on (the default) it is told when an agent it
started finishes, and answers in its chat; --follow off tells it nothing. --chat-apps away (the default)
sends its answers to a paired Telegram or Discord only while you are away, always sends them, off
never; a message you write to it from there is answered there either way. With --json it prints the
bot as `bot list --json` does; its state is `thinking` while it sets itself up.

Example:
  cornercase bot new prs --agent claude --about \"Review my open pull requests and tell me which are blocked\"";

pub(in crate::cli) const BOT_ABOUT_HELP: &str =
    "Prints what the bot is for: the text of its bot.md, as `bot new --about`, `bot edit --about` and the
window's form write it. The bot changes it too, when you give it a job or change one, so it is read
when you ask. An empty or missing bot.md prints nothing; one that cannot be read is an error.

With --json it prints {\"bot\", \"about\"}. A program's edit form shows `about`, and sends it with `bot
edit` only when the person changed it.";

pub(in crate::cli) const BOT_EDIT_HELP: &str =
    "Changes what the bot uses from its next message on; a running answer is not changed. An empty
--model, --effort or --account goes back to the agent's own; --about writes bot.md (`bot about` prints
it); --follow on|off sets whether the bot is told when an agent it started finishes; --chat-apps
away|always|off sets when its answers reach Telegram and Discord, at once. The agent cannot
change, because another agent cannot continue the conversation: make another bot instead.";

pub(in crate::cli) const BOT_SAY_HELP: &str =
    "Writes to a bot and starts a turn: the agent runs without its window, continuing the bot's
conversation, and its answer lands in the chat. The command answers at once with the line's id; with
--wait it prints the bot's answer when the turn ends, and exits 3 when the turn fails, is stopped
or times out (15 minutes), with the reason on standard error.

A message sent while the bot answers is held and goes, with the others held, as the next turn. The
first message of a conversation opens with what cornercase tells the bot about itself, then bot.md
and memory.md. A bot runs with CORNERCASE_BOT set, so its `cornercase` commands must name the
project: it runs in no pane (`queue add` with no place means the bot).

With --json it prints {\"bot\", \"line\"} (and \"asked\" with --wait): the chat line.

Examples:
  cornercase bot say prs \"which of my PRs are blocked?\" --wait
  echo \"summarize this\" | cornercase bot say prs - --wait";

pub(in crate::cli) const BOT_HISTORY_HELP: &str =
    "Prints a bot's chat as you see it: what you wrote, what the bot used (`▸ cornercase status --json`)
and its answers. --after LINE and --last N cut it.

With --json it prints {\"bot\", \"name\", \"lines\": [{\"id\", \"at\", \"kind\", \"text\"}]}. A kind is you, bot, step,
error or notice; a you line may be held, a step names its tool, a bot line has its model, tokens
and conversation, and a line of a routine's run has routine, the run's queue id; a setup turn starts
with the notice `setting up` in place of a you line.";

pub(in crate::cli) const BOT_HELP: &str =
    "A bot is an agent of your own, outside any project: a name, a folder, a memory and a chat. Each
message you write runs one turn of its agent (one `cornercase bot agents` lists as able to be a bot) in a conversation that continues,
and the answer lands in the chat. It can see every project and act on them through `cornercase`.
A bot made with --about sets itself up at once; after that it spends tokens when you write to it and when its
routines run, and only while the server runs. The window lists bots in its Bots section and draws each one's chat.

Events: `cornercase events` reports each chat line, the bot starting or finishing a turn, and a
bot being created, edited or removed. `cornercase status --app` carries the list as `bots`.

Example:
  cornercase bot new prs --about \"Review my open pull requests\"
  cornercase bot say prs \"which of my PRs are blocked?\" --wait
  cornercase bot history prs";

pub(in crate::cli) const BOT_SETUP_HELP: &str =
    "Runs the bot's setup turn again, as `bot new --about` runs it when it makes the bot: a new
conversation that opens with what cornercase tells the bot about itself, bot.md and memory.md, and
asks it to schedule what bot.md asks for and say what it will do. Its chat shows `setting up` as the
turn's first line, and the next message continues that conversation. Use it after a setup failed or
was stopped, or after changing bot.md with `bot edit --about`, which runs nothing by itself.

It answers at once with the id of the `setting up` line. It is refused while the bot answers, and
when bot.md is empty. With --json it prints {\"bot\", \"line\"}.";

pub(in crate::cli) const BOT_SECRET_HELP: &str =
    "A bot's secrets are named values, a password or a token, that cornercase keeps encrypted for that bot
and that only that bot's own turn reads, with `cornercase bot secret get NAME`. A name takes 1 to 40
characters of a-z, 0-9, - and _; a value 1 to 8,192 bytes; a bot keeps at most 20. A bot that lacks one
asks the user for it with `cornercase bot secret ask NAME --about TEXT`.";

pub(in crate::cli) const BOT_SECRET_SET_HELP: &str =
    "The bot reads this value: it reaches the bot's agent and stays in that agent's own records on this computer.

Reads the value from standard input, never from an argument, and replaces a saved one; one line break at
the end is dropped, the rest is kept as given. On a terminal it asks for the value without showing it.
It prints the name. With --json it prints {\"bot\", \"name\", \"saved\"}. A bot cannot run it.

Example:
  printf '%s' \"$PASSWORD\" | cornercase bot secret set campus campus-password";

pub(in crate::cli) const BOT_SECRET_LIST_HELP: &str =
    "Prints the names and when each was saved, never a value, and where the key is kept (`key: file`).
Inside a bot, BOT is that bot by default, and a bot lists only its own.

With --json it prints {\"bot\", \"key\", \"secrets\": [{\"name\", \"saved\"}]}. When the file does not
open (another key, a damaged file) it says so and changes nothing: only `bot secret remove BOT --all`
starts again.";

pub(in crate::cli) const BOT_SECRET_REMOVE_HELP: &str =
    "Removes one secret, or with --all every secret of the bot, even a file that does not open. A bot
cannot run it. With --json it prints {\"bot\", \"removed\"}.";

pub(in crate::cli) const BOT_SECRET_GET_HELP: &str =
    "Prints the value and a line break. Only the bot itself may run it, from its own turn: every other
client, the user's own shell too, is refused. Never write the value in an answer, in bot.md or in
memory.md. With --json it prints {\"bot\", \"name\", \"value\"}.";

pub(in crate::cli) const BOT_SECRET_ASK_HELP: &str =
    "Only the bot itself may run it, from its own turn. It never waits: the user sees `asks for a secret` in
the bot's chat, a dialog with a masked field for each name (or a notice with ` go `), and types the
values there; the bot is written to once they are saved. A value never goes through the chat.

1 to 4 names; --about is why, in one line of 1 to 200 characters, shown to the user as the bot's words.
A name already saved is refused without --replace, the same names asked again answer `already`, and a
bot has at most 4 asks waiting. A restart forgets the asks. With --json it prints
{\"bot\", \"ask\", \"names\", \"already\"}.

Example:
  cornercase bot secret ask campus-user campus-password --about 'to read your grades on the campus site'";

pub(in crate::cli) const BOT_SECRET_GIVE_HELP: &str =
    "Reads one JSON object from standard input, each name the bot asked for to its value, and saves them all
or none. A bot cannot run it. It prints the saved names. With --json it prints {\"bot\", \"ask\", \"saved\"}.

Example:
  printf '%s' '{\"campus-user\":\"…\",\"campus-password\":\"…\"}' | cornercase bot secret give campus 3";

pub(in crate::cli) const BOT_SECRET_DECLINE_HELP: &str =
    "Ends the ask without saving anything; the bot is told at the next message in its chat. A bot cannot run it.
With --json it prints {\"bot\", \"ask\"}.";
