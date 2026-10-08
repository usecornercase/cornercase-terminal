---
name: cornercase
description: Drive cornercase, the terminal multiplexer you may be running in, from its command line. Use it when CORNERCASE=1 is set and the user asks you to open tabs or splits, run something in another pane, or start, wait for and coordinate other coding agents, each in its own git worktree if needed.
---

# cornercase

When the environment has `CORNERCASE=1`, you run in a pane of cornercase, a terminal multiplexer. The `cornercase` command drives it: it lists projects, workspaces, tabs and panes, opens tabs and splits, types into other panes, starts coding agents in their own git worktrees, waits for them and reads what they wrote.

- Use it only when the user asks you to open tabs, run something in another pane, or start and coordinate agents, and only when `CORNERCASE=1`.
- Learn the syntax from `cornercase --help` and `cornercase <command> --help`. Never guess flags. `cornercase status` lists the ids; your own pane is marked `(you)`.
- To act on a worktree you started, name it by its branch with `--worktree BRANCH` where a command takes `--workspace ID` (`cornercase close --worktree fix/login --remove-worktree`) instead of reading its id from `status`.
- Never steal focus: no `cornercase focus` or `--focus` unless the user asks. Never run `cornercase kill-server`. Close only what you created.
- Do not answer another agent's permission prompt or question (status `waiting`) without asking the user.
- An agent shown as `working (background shell)` has ended its turn but left a background command running, and may wake up when it ends; `--until turn-over` stops waiting there. Read its pane, and stop that command if nothing should wake it.
- After a timeout, or `send --enter` failing with `not confirmed`, `cornercase read` the pane before sending anything again: the agent may have received it.
- `send` refuses a Claude Code agent showing a dialog, a panel or its shell mode instead of its input box (`dialog open` in `status`). Read the pane; use `--force` only once you know where the text will go.
- Keep `--timeout` under the timeout of your own command and wait again, or run the wait in the background.
- To follow several agents, run one `cornercase wait --any` (or `--all`) with a `--pane` for each, not one wait per pane; `--any` prints the id of the one that ended.
- To react to each change as it happens, run `cornercase events --pane ID…` in the background, or with a tool that streams a command's output: it prints a line each time one of those agents changes state, a program in its pane ends or the pane closes.
- For long results, ask the other agent to write them to a file, then read the file.
- When cornercase itself misbehaves (a command fails oddly, a tab or agent does something unexpected, "cornercase hit a bug"), `cornercase logs` prints what the server did, step by step, with its errors; quote the relevant lines to the user.
