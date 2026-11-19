# Bots on Telegram and Discord (#848)

Code: `app/messenger/bots.rs` (sending a bot's lines, replies, `/bots` and `/bot`), `app/messenger/forum/posts.rs` (`Owner`), `app/messenger/forum/posts/saved.rs` (a bot's post in `discord.json`), `bots/mod.rs` (`ChatApps`), `bots/form.rs` and `ui/dialog/bot.rs` (the form's row), `messenger/mod.rs` (`Tap::Bot`, `COMMANDS`, `ARGUMENTS`), `discord/mod.rs` (the slash command's options).

A bot's answers reach the paired Telegram and Discord (see [Telegram](../telegram.md), [Discord](../discord.md)), and you can write to a bot from there. Before this, a bot spoke only in the window and in `bot history`.

## The option

`chat_apps` in `bot.json`: `away` (the default), `always` or `off`, written only when it is not `away`, so a `bot.json` from before keeps its bytes. It is set with `bot new` and `bot edit --chat-apps`, the `bot` request's `new` and `edit` (`chat_apps`, a string, checked by the server: `the chat apps option is away, always or off`) and a row of the bot form. `bot list --json` and `status --app` give it as `BotInfo.chat_apps`; the contract feature is `bot-chat-apps`, and the command line checks it only when the flag is given, as it does `bot-follow`. It applies at once. `bot edit` saves `bot.json` before it changes the live bot and refuses with `failed` when it cannot, so a setting that would not survive a restart is never shown as taken.

**The default is `away`**, the issue's own proposal: away is switched by hand, and an answer at the desk already makes a toast and a notification. `always` is for a bot you mostly talk to from the phone; `off` for one whose routines would be noise there.

The form's row is a list only while a chat app is paired (`chat_apps_ready`: any `Messenger::paired()` when the form opens); otherwise it is a grey line, `pair Telegram or Discord in Settings › Remote`, that takes no focus, and the form sends no `chat_apps`, so saving a bot with nothing paired keeps its value. The row is always listed, so the rows below it do not move when a phone is paired. The label `Chat apps` fills the label column; the snapshot showed one space before the field, so it was kept rather than shortened.

## What is sent

Only `bot` lines (a chat answer, the setup's, a routine run's, an answer to a following report of #847) and `error` lines. `you` (you know what you wrote), `step` (too many, too small), `started`, `finished` and the notices are not sent. A line is sent as it was written to the chat, after `bot_append`, so a secret is already redacted.

- **The title** is `🤖 <name>` in bold, and ` · ↻ <label>` when the line belongs to a routine's run (the label cut at 120 characters). The body is the answer's Markdown through the platform's `Markup` (`markdown`, then `say_long` splits it by its limits, the header joined to the first part when it fits); an `error`'s first line starts with `✗ `, the issue gave no format.
- **Unasked**, a line goes to a platform that is paired when the option is `always`, or `away` and that platform is away.
- **An answer to a message written from a chat app always comes back there, as a reply to it**, even with `off` or while not away. The option governs what a bot sends on its own; a 👍 and then nothing would read as broken. The reply is found by the `you` line the message made: `bot say` answers with its id, and an answer whose turn took that line (`Turn.asked`, the held lines included) is sent as a reply to the newest such message, in the chat it was written in. The waiting messages are not capped, since an evicted one would lose its answer: each leaves when its answer is sent, and a removed bot's leave with it. An answer whose chat is no longer heard (the phone was forgotten or another one paired meanwhile, or the post is gone) is not a reply, so it never lands in another chat; it follows the option like an unasked one. The notice that drops held messages (`stop`) goes back the same way to the messages it dropped.
- **A failed run's `error` line replaces the queue's away announcement** for a bot's entry (`bot_entry_failed` called `announce`). Otherwise a failed run would be sent twice while away, and the announcement ignored the option; the toast, the notification and the `queue-failed` message stay.
- **A `waiting` line** (#847) sends the agent's own notice, its question and buttons (`notice_waiting`), when the option is `always` and the platform is not away. While away, `watch_messenger` already sends that agent's notice, so the bot adds nothing and it arrives once.

The secret asks of #878 keep their own `announce`, unchanged.

## Writing to a bot

- **A reply to a bot's message** goes to that bot. The message is remembered with `Route::Bot(key)` (the bot's `Live.key`, per server run) like any routed message, and the reply becomes a `bot say` request from the chat's internal client, so its refusals (`not-found`, an empty message, more than 20,000 characters) and its log line (`bot say bot=… bytes=…`, never the text) are the command's own, and a refusal comes back as a reply. A taken message gets 👍.
- **A reply to a routine's answer is a plain `bot say`**: the chat's turn gets the answer in front by #846's `unseen` rule when the chat was not told yet. It does not resume the run's session; the issue says the chat.
- **`/bots`** lists every bot (at most 25, then `… and N more`): `🤖 <name> · <answering|idle>[ · N unread][ · next <time>]`, the time in the queue's zone, with one `✎ <name>` button each (`Tap::Bot(key)`, data `<run>:bot:<key>`, so an older run's button is stale like every other) that asks what to tell it. No bots: `No bots yet: make one in cornercase, in the Bots section.`
- **`/bot NAME TEXT`** writes to the bot. The name is matched as the `bot` request matches it (id, else name ignoring case), trying the longest head first, so a name with spaces works and `my bot` wins over `my`. An unknown name answers with the list under `No bot has that name.`; a name alone asks for the text; `/bot` alone lists. Discord registers the two arguments as required string options (`ARGUMENTS`), since its client completes a typed `/bot` and would otherwise send no text; `slash_command` puts their values back after the name, each on its own line, and `/bot` takes a first line as the exact name: joined by spaces, `name=My`, `text=bot hi` would have gone to a bot called `My bot`. A bot's name holds no control characters, so the line cannot be part of it. Telegram takes the text as typed. A Discord slash command is an interaction, not a message, so its 👍 finds nothing to react to (logged like any undelivered order) and the answer's reply reference is dropped by `fail_if_not_exists: false`: the answer still comes, in the DM.
- A message to a removed bot answers `That bot is gone. /bots lists the bots now.`

## Forum mode

**One post per bot, titled `🤖 <name>`**, beside the workspaces' posts. Posts are keyed by an `Owner`, a workspace id or a bot's key; rejected were a second `Posts` for bots (every link event would look in both) and offsetting bot keys into the workspace ids (cleaning would take a bot for a closed workspace). A bot's post is tagged `needs you` while it has an ask (#878), `working` while it answers, else `done`.

**A routine's answer goes in the bot's post**, with its `↻ <label>` title, following "one post per workspace, not per agent": a post per run would add a post a day per routine and count against the server's active threads, and a reply to it goes to the chat anyway. An answer to a message is a reply where the message was written (the post or the DM); a multi-part answer resolves that place once, so its later parts do not land in the post. A message you write in a bot's post goes to that bot (a reply to one of its messages, or plain text); a command there answers that commands work in the direct message. Removing the bot closes its post as a closed workspace's is (`discord_forum_close`).

**A bot's post is kept in `discord.json`** (`bot_posts`, bot id → thread), since a bot's id is stable across restarts and a workspace's id is not (workspace posts stay in `session.json`). It is rebuilt and saved when a post is made, gone or cleaned, only once the server keeps bots, and restored when the bots load; a new forum forgets it.

## Not gated

A bot's own connection may change its own `chat_apps`, like its name or `follow`: it decides when its answers reach a chat you paired, not what the bot can do. Gating it like `access` would be one arm in `app/control/boundary.rs` and `cli/guard.rs`.

## What was checked

Against fakes only: the captured link of the app tests for Telegram and Discord (the option, replies, `/bots`, `/bot`, a waiting agent, a failed run, forum posts and their restart) and the fake HTTP server for Discord's command list. No real Telegram or Discord was used; whether Discord accepts two required string options on a user-install DM command was not tried live.
