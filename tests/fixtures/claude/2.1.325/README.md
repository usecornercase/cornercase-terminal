# Claude Code 2.1.325 fixtures

Claude Code 2.1.325, Linux, 2026-11-14.

- `session-shell.json` is a live session file of a turn that ended with a background shell running, with the pid, ids, paths and name replaced.
- `background-shell.txt` is the sequence of OSC 7501 reports (the payload after `ESC ] 7501 ;`, `-` for none) paired with the status the session file says at that step. It was rebuilt from 2.1.325's code, not captured from a terminal: a turn starts `cargo test` with `run_in_background` and ends (`done`, file `shell`), the shell ends and its notification is queued (`working`, file `idle`), the wake-up turn runs (`busy`, Claude sends nothing) and ends (`done`).
- The transcript is not read for this; its shapes are in `design/agent-status.md`.
