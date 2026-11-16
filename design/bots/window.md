# Bots › The window

Part of [Bots](../bots.md). Code: `app/bots/` (`shown.rs`, `window.rs`, `section.rs`, `talk.rs`, `menu.rs`, `form.rs`, `routines_dialog.rs`), `ui/bots.rs`, `ui/bots/chat.rs`, `ui/bots/card.rs`, `ui/dialog/bot.rs`, `bots/form.rs`, `app/notifications.rs`, `control/status.rs` (#845).

#844 made bots with no face but `cornercase bot …`. The window draws a bot **as a chat** (bubbles, Markdown, what it is doing), never as the agent's interface, since #844 runs the agent without it and keeps the chat in `chat.jsonl`.

## Where the chat shows, and what leaves it

- **In place of the panes.** A click on a bot's row shows its chat in `Areas::pane`, and the workspaces column shows the bot's **card**. The tab bar stays above (`tabs: top`), the panes keep their size (`App::layout` does not change), and a click on a tab goes back. `App::tab_view` returns `None`, `App::visible_tab` and `App::shows` say no pane is on screen, so agents in the hidden tab notify as in any other hidden tab, and PTY output does not redraw for nothing.
- **What leaves it**: every move to a local place calls `App::leave_remote`, which now also calls `leave_bot`: a click on a project, a workspace, a tab or an agent row, `focus`, a notification click on an agent, the keys menu's moves, the picker and the search. Picking a machine's workspace (`select_on`) leaves it too. Showing a bot leaves a machine's workspace the same way. A bot being removed leaves it and closes any dialog naming that bot.
- **Shared by every window, not by views, and not saved.** The shown place is `App` state mirrored by every attached window (`design/client-server.md`), so the shown bot is too (`Bots::shown`). A view (`cornercase view`) keeps a selection of its own and never shows a bot. After a restart the window shows the panes, as it does for a machine's workspace (`shown_remote`).
- **On the socket**: `Report.shown_bot` (feature `shown-bot`), not a field of `Ids` or `surface`: see `design/commands-for-scripts-and-agents/status.md`.

## The section

The sidebar section, its placement, height rule, saved fold and compact page are in [Layout › The agents section](../layout/agents-section.md) and [Layout](../layout.md); the saved fold is in [Saved session](../saved-session.md). What belongs to the window: bots keep their `bot list` order, not their status (rows that move are hard to click), and there is **no icon in the icon bar**, which holds panels and tools, while bots are a sidebar list.

## The chat

- **Lines**: only the shown bot's last 1,000 lines are in memory (`Chat.lines`), read when the chat is shown and then appended by `bot_append`; `cornercase bot history` reaches older ones. A `you` line is a bubble on the surface colour, right-aligned, at most 80 % of the width, with its time or `held` while its id is in `Live.held`. A `bot` line has a dim `name · time` above its Markdown (`highlight::markdown::render`, code blocks highlighted as in the files panel). A run of steps folds to its last 3 under a ` +N steps ` that a click unfolds (kept per chat by the run's first step). An error is red with ` retry ` when it is the last line and no turn runs, which sends the last `you` text again, or runs the setup again (`bot setup`) when the last line that opened a turn is `setting up`; another window does the same with the `setup` action, having no `you` line to send. A notice is centred. While a turn runs the last row is the spinner, `thinking · 42s · 3 steps` and ` stop `, and `stopping…` after a stop.
- **Rendering cost**: rows are laid out per entry from the newest, only as far as the view needs (`scroll + height`), and the Markdown of each answer is cached per line id and width (`Chat.bodies`, cleared when the width changes). A 60 fps draw never lays out the whole chat. The answer's code fences are highlighted ahead, in the turn's job (`markdown::warm`), so a first paint never runs the highlighter on the server's loop; a chat loaded by `show_bot` is not warmed (not measured).
- **The view** is anchored at the bottom; `Chat.scroll` counts the rows above it and 0 sticks. A line that arrives while it is above 0 adds its height to `scroll` so the text read stays put, and sets the ` ↓ new ` mark, which a click or reaching the bottom clears. The wheel scrolls as in a pane (`design/scrollback.md`).
- **The empty state** (a chat with no lines, which is a bot made without For: a setup's notice is a line) says what a bot is in two lines and offers three examples; a click fills the field and sends nothing. None promises a schedule (#846 adds that example).
- **Times** use the window's zone (`App::window_zone`, the queue form's): `HH:MM` today, else `Mon 3 Nov 14:02` (`queue::clock::when`, shared with `cornercase bot history`).
- **The header** has ` routines ` one cell left of ` new conversation ` (10 cells, cyan, enabled while a turn runs); it opens the bot's routines dialog (see [Bot routines](routines.md)). It is left out when keeping it would leave the name under 12 cells, and ` ⋯ ` still offers `routines…`.
- **` new conversation `** while a turn runs is dim, and a click says `PRs is answering: stop it first` (#844's `busy` refusal); the menu greys it.
- **The card** (workspaces column): the name, agent, model (`the agent's own` when unset), effort and account when set, the access (`full access` in red), `made 2026-11-14`; the folder comes right after the name, since it is a click target (a click opens the open-in menu, or copies the path), and the two rows of a routine are the other (a click opens its routines dialog, selected; the value turns cyan while hovered) and the stacked layout gives the card few rows. The tree layout has no workspaces column, so it has no card.

## Followed agents in the window (#847)

The chat draws `started`, `finished` and `waiting` lines (`ui/bots/agent.rs`) as one entry each: a header with the glyph in the status colour, the text and ` show ` (cyan, `RowHit::Show`, which leaves the chat through `jump_to_pane`) or a dim `closed` for a pane that is gone or a line from another run of the server; under a `finished` its last message as Markdown (cached by line id like answers) and under a `waiting` the screen excerpt as plain text, both dim and indented, six rows and then ` +N lines ` (a click unfolds, keyed by the line id). The card lists the bot's live tagged agents under `started`, with the status icon, and a click on a row shows that tab (`card_row_at`). `by <bot>` is in the details line of tab rows, the tab bar and agent rows. The form's **Follow** row is a `[x]` checkbox like the queue form's cleanup (`Space` or a click toggles it) and makes the dialog 20 rows high. See [Following](following.md). The simulation's canned chat shows a started and a finished pair; ` show ` does nothing there.

## Routines in the window (#846)

A routine run's lines are drawn as one block, the section's row shows `next 09:00` while the bot is idle, the card lists the routines, the empty state has a fourth example and the remove dialog names the entries it removes: see [Bot routines](routines.md) (The window). The simulation shows a canned run block and a seeded routine, and runs none; a bot made in its form with For filled in plays a canned setup that schedules a routine. Its bots' entries live in `app.botQueued`, not `app.queued`: the landing's scripted worlds and `queue.test.ts` need an empty project queue, while the card, the sidebar's `next` and the routines dialog need the bot's routine in them too. `pauseQueued`, `removeQueued` and `removal` take the store to act on, so the dialog reuses the Queue tab's code and its undo toast.

## Keys

While a chat shows, every key goes to its input field and **never to a pane**: `chat_typing` is true when a bot shows, no compact menu or full-screen file is up, and the TODO field, the files search, the changes filter and the tour do not have the keys (they keep them). `forward_key` and `handle_paste` also return early while a bot shows, so no path can write to a hidden pane. The prefix key still opens the keys menu. `Enter` sends (any modifiers); a line break comes only from a paste (`Editor::insert_lines`); the reason is in `design/text-fields.md`. `Esc` clears a draft with text; with an empty one it closes a side panel when one shows, and otherwise does nothing: going back to the last tab would give `Esc` two meanings depending on the text, and a stray `Esc` would drop the place you are reading. The field is `TextField::BotChat` in the registry, so drags, double clicks and the copy/cut/select-all menu work as in every field. Drafts are per bot and shared by windows, like every field in `App`.

## Unread and ` go `

- An answer or error of a bot whose chat a window shows on screen (`bot_on_screen`: shown, a window attached, no compact menu, no full-screen file) neither counts as unread nor notifies; showing a chat does what `bot seen` does. Any other keeps #844's toast and desktop notification, which now carry a token (`Notices::bot_token`, bounded like the agents' tokens): the toast's ` go ` and a click on the desktop notification show the chat, or toast `that bot is gone`. `act_on_notice` and `cornercase go` are unchanged: a bot's token is not published on `events`, whose `bot` events already carry its lines.

## The form, rename and remove

- **The form** (`bots/form.rs` is the model, `ui/dialog/bot.rs` the drawing): Name, Agent, Model, Effort, Account, Access, Follow and What it is for. The agent list is `bot agents`' (`bot_agent_infos`), with the agents that cannot be bots greyed with their `why` (see Dialogs › Disabled options and typed values); the preselected one is `config.agent` when it can be a bot. The model is a combobox that also takes a typed name, `default` meaning the agent's own. Effort shows only for an agent that takes one; Account only when the agent has accounts, as in the queue form (`AgentAccounts`), with the bot's own login preselected when editing. **Access is never chosen for the user when `cornercase` is not offered** (Codex today): ` create ` answers `choose its access` under the form without sending. The note under it has two lines, the full access one in the error colour. ` create ` shows the chat, where a bot made with For sets itself up (#887), and ` save ` of a changed For runs nothing. Server refusals (name taken, 50 bots) show under the form, which stays open; any key clears them.
- **`edit…`** is the same form with ` save `, the agent grey and skipped by `Tab`. It sends `about` only when the text changed, through `EditBot.about` (contract feature `bot-about`; `bot edit --about TEXT` for programs, checked by the CLI before sending), which writes `bot.md`.
- **`rename`** is the small name form (a refusal shows in red in it). **`remove…`** asks `Remove bot prs? Its chat, its memory and its folder are deleted. Agents it started keep running.`; while a turn runs its button says ` stop it first ` and stops the turn, keeping the dialog open, and reads ` remove ` again once it ended: the reading where it acts saves a trip to ` stop `.

## Links

A click on an `http://` or `https://` address in a bot's line **copies** it; why is in [Opening in other apps](../opening-in-other-apps.md). Opening it is a follow-up issue.

## Compact mode and the phone

The compact page is in [Layout](../layout.md). The browser (`web/`) draws the window, so a phone gets the chat; typing from a phone was not checked live by anyone, and an e2e test through the browser bridge stands in for it.

## What was checked, and what was left out

Everything runs against fakes (`fake_program`, `FakeClaude`); no real agent ran. Left out: selecting text inside the chat (the terminal's own `Shift`+drag works), `focus --bot`, an icon-bar icon, a landing-page card, keyboard scrolling of the chat, and editing `memory.md` from the window.
