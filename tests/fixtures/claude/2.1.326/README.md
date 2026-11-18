# Claude Code 2.1.326 fixtures

Claude Code 2.1.326, Linux, 2026-11-17, rendered in tmux (`capture-pane -p`, trailing spaces trimmed) with a scratch `CLAUDE_CONFIG_DIR` and a fake API key; folder paths replaced.

- `add-dir.txt` is the start-up question about a folder given with `--add-dir`, in a 100x40 pane, with `No, exit` selected. `Down` then `Enter` picks `Yes, add it`.
- `folder-trust.txt` is 2.1.326's question about the folder it starts in (`Accessing workspace:`), shown first in an untrusted repository.
- `add-dir-cut-off.txt` is the added-folder question in a 45x14 pane, too short for it: its top scrolled off and Claude Code printed ` Adding: <folder>` under it.
- `resumed-quote.txt` is a resumed Claude Code (no `--add-dir`) whose last reply quotes the question, above its input box.
