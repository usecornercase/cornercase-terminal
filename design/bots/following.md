# Following the agents a bot starts (#847)

A bot that starts an agent (`cornercase start`, `new-tab` or `split` with an agent command, `flow run`) is told when that agent's turn ends, and answers in the conversation that started it. The pane remembers which bot started it, the chat says so, and a `finished` line carries the agent's last message.

## Tagging

- **The tag is `Term::bot`**, a bot id, saved as `PaneState.bot` (optional, skipped when empty, so a session without tags is written byte for byte as before; `VERSION` stays 5). It is kept at restore only when its bot still exists (`restore_tab`, so closing and reopening an agent carries it too), and carried across an account restart (`respawn_in`). Removing a bot clears the tag of every pane of that bot.
- **The CLI says which bot asks in the request's envelope**, beside `caller`: `bot` (from `CORNERCASE_BOT`) and `bot_run` (from `CORNERCASE_BOT_RUN`, set by a turn that continues or is a routine's run). The issue put the pair on `start`, `new-tab`, `split` and `flow run`; the envelope was chosen because a `send` from the bot has to re-arm the follow and the server marks only the connections of `cornercase`-access bots (a `full` bot's is never marked), so the request itself has to say which bot sent it, and because `control.rs` is at its 1,000 lines. `bot` and `bot_run` are in `NULLABLE` of the strict-fields check, so a client that writes `null` is not refused. The envelope leaves them out under `--remote` (a bot on another machine is left for later, and a far server must never tag a pane with the bot of the same id there).
- **Who is the bot** (`App::starter`): the connection's bot when the server marked it, else the envelope's. A name that is no live bot gives no starter. `bot_run` is kept only when that queue entry exists and belongs to that bot. So a `cornercase` bot that names another bot tags with its own, and the label never widens what a connection may do. A `full` bot and the user's terminal are taken at their word: both can already `bot say` any bot.
- **`Asker.bot` is a `Starter`** (the live bot's key and the run), so `Asker` stays `Copy`. `new-tab`, `split`, `start` (also with `--worktree`, through `Then`, and for an issue, through `Asking`) and the orchestrator of `flow run` (`Starting.bot`) tag. Setup and login tabs and anything relayed with `--machine` do not.
- **Following off still tags** (`by prs`, `status`): where an agent came from is a fact. What the option turns off is behaviour: no lines, no follows, no turns.

## The follow list and arming

- A bot follows at most **20** panes (`MAX_FOLLOWED`); a 21st drops the oldest with the notice `no longer following <agent> in <place>: a bot follows at most 20 agents`. The limit counts follows, not tags: a dropped pane keeps `by prs`.
- **Armed** means the next end of the agent's turn is reported. A follow starts armed unless the request waited (`start --wait`: the bot reads the result in that turn and a report would repeat it). Only the first end after each arm counts: arming is the start, or a later `cornercase send --enter` from that bot, without `--wait`, into the pane (the hook is `put_through`). What the user types in the pane changes nothing, so answering a question after a `waiting` line does not lose the finish, and talking to the agent yourself stays quiet.
- **The turn rule is the chat apps' rule** (`app/settle.rs`, moved from the Telegram code): the state holds for one second and busy is remembered, so a flicker is not an end. `follow_bot_agents` runs it for each followed pane from `refresh_agents`. A `waiting` line is written when armed, with the question's excerpt (the last 15 screen rows), starts no turn, and counts as unread without a toast, since the agent's own `needs you` notice exists.
- **A flow's orchestrator** (D3) ends many turns during one run. Its end counts only when its run's end is marked (`Run::ended`) and its turn ended, or when the run has stood still (`Run::stopped`) for 60 s. Otherwise the first background wait would report a run that is still working, and the real end would never be told, since the bot does not prompt the orchestrator again. A flow's other members are never tagged.
- **After a restart** every follow is rebuilt from the tags, **disarmed**: every turn the bot waited on was cut by the restart, and a resumed agent's own turn would look like a finish. The next `send --enter` from the bot arms it again. Old chat links read `closed` because pane ids restart from 1 (the line's `boot` tells).

## Reports and turns

- **The read.** When an armed follow ends, the last message is read from the agent's record in a job (the same read as `last-message`), else the screen excerpt is used. The body is cut to 2 KB on a character boundary (`… (cut)`).
- **A `finished` line is written when the turn that reports it starts** (D2), as that turn's prompt line, the way `you` is, so `chat::replay` stays correct: it reads `finished` exactly as a `you` that is not held. Reports wait in memory for 30 s after the first, and for the bot's single turn slot, so a restart loses the reports still waiting. Rejected: writing at once and starting a turn without a prompt line (replay could not tell a report turn ran), writing `held` lines while idle (breaks replay's model), and joining reports into the user's held turn (impossible for a run's session, and it mixes sessions).
- **One slot.** Report turns use the same slot as chat turns, so two processes never share a session and the steps stay readable. Routine runs stay concurrent, as before.
- **Which conversation.** The one that last armed the follow. A chat turn (or the start from a chat turn) → the chat's session. A routine run (`bot_run`) → the run's session (`State::Answered.session` on the queue entry): the turn resumes it, `Session::Resume`, and its answer is a normal `bot` line with `run` set, which is also told to the chat's next turn (`unseen`). A run that is still answering holds its reports until it has answered, and one without a session (failed, removed) uses the chat's. Several reports of one conversation that come within 30 s go into one turn, up to 16 KiB; the others wait.
- **The prompt** says `cornercase says` instead of `The user says` when it is reports only (`Input.from_cornercase`). The brief's last section is `FOLLOW` when following is on and `NOT_FOLLOWING` otherwise.
- `stop` drops pending reports silently (nothing was written for them); `remove` takes the `Live`, its follows included, with it. `follow off` clears the follows and the reports; `follow on` rebuilds from the tags.

## The window

- Chat lines `started` (` show ` or `closed`), `finished` (the last message as Markdown, six rows and ` +N lines `) and `waiting` (the screen excerpt, plain). ` show ` is `RowHit::Show(pane)` and leaves the chat through `jump_to_pane`; a pane that is gone, or a line from another run of the server, reads `closed`.
- The bot's card lists its live tagged agents (`started`, with the status icon); a click shows that tab. `by <bot name>` shows in the second line of a tab row, the tab bar and an agent row, after the agent kind and before the model, which gives way first.
- The form has a **Follow** row (`[x] tell it when an agent it starts finishes`).

## For programs

`status`, `watch` and `view` carry `bot` on tabs and panes (left out when none); `bot list --json` and `status --app`'s `bots` carry `follow` and `following` (pane ids); the three line kinds come in the `bot` events with `agent`, `body` and `run`; `bot new` and `bot edit` take `follow`; `cornercase contract` lists `bot-follow`. The contract stays 1: everything is optional and additive. The CLI checks the feature only before `--follow`.

## Checked

With fakes only (a `FakeClaude` as the started agent, a Gemini CLI fake as the bot, a virtual clock through `refresh_at`); no real agent ran, and nothing was tried on macOS or Windows beyond the tests that run there.

## Left for later

Telegram and Discord (#848); an agent that answers its started agents' questions (no turn on `waiting`); following agents started on another machine; a `bot` column in `status --panes`; remapping old chat links to restored panes after a restart; following the children an agent starts.
