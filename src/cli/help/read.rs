pub(in crate::cli) const READ_HELP: &str =
    "Lines the terminal wrapped come back joined, and empty lines at the end are left out.

With --last-message it prints the last message the agent in the pane wrote, as plain text, from
its own record (Claude Code's transcript, Codex's rollout, Gemini's session file, opencode's
database, Grok Build's session, Cursor CLI's chat store, GitHub Copilot CLI's session events, Vibe CLI's unified session store) instead of the
screen, so it comes whole even once it scrolled off, without the input box or status lines. While the agent works
it is the newest one so far. --json adds when it was written and whether the agent's turn is over. It fails on a
pane without Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI, Vibe CLI or GitHub Copilot CLI, and before
the agent wrote anything.

With --last-message, repeat --pane or --tab to read several agents at once. Each message has a
header with its pane, agent, turn_over and written time; --json gives {\"panes\": [...]}. Panes come
first, then tabs, in the order given, and a repeated id is read once. Errors stay with their target,
and it exits 3 after reading them all if any failed; with no server it prints nothing and exits 1.
One target keeps the output above. Reading the screen takes only one target. With --boot, a server that
restarted since ends it with 4 before anything is printed, with several targets too.

With --pending it prints what the agent in the pane waits for you to answer, from its own record:
each question with its numbered options, or the tool and command of a permission. --json gives it as
data (questions, options with label and description, multi_select, free_text), and null when nothing
is pending. selected and open are null: only the screen knows the chosen option and the open
question. Only Claude Code's records say it; another agent or a shell fails with the reason. Repeat
--pane or --tab as with --last-message; a target with nothing pending is {\"pane\": N, \"source\": null}.
Answer with `cornercase keys`.

With --messages it prints the conversation of the agent in the pane, oldest first, from the same
records as --last-message: what the user and the agent wrote, without tool calls, their results or
thinking, each under a header with its id, role (user or assistant) and, when the record has it, the
time it was written. A prompt typed in the pane and one sent with `cornercase send` are both user
messages. --last N gives the newest N [default: 20]; --before ID the N just before that id, to page
back; --after ID every message after it, or the newest N of them with --last. Without --json, a line
on stderr names the --before that reads the page before. --json gives {\"pane\", \"agent\",
\"conversation\", \"messages\": [{\"id\", \"role\", \"text\", \"written\"}], \"more_before\"}. An id is the
message's position in the conversation, from 1, and stays while the record only grows; when
conversation changes (a /clear, another session), ids start again. It takes one target, and fails
as --last-message does.

Claude Code's feedback survey (`How is Claude doing this session? (optional)`, then
`1: Bad  2: Fine  3: Good  0: Dismiss`) may show above its input box: it is Claude Code asking you,
not the agent's output. `cornercase keys --pane N 0` dismisses it.

Examples:
  cornercase read --pane 12
  cornercase read --pane 7 --lines 200 > build.log
  cornercase read --pane 12 --last-message
  cornercase --remote vps read --pane 9 --last-message --boot 4242-1700000000
  cornercase read --tab 4 --last-message --json
  cornercase read --last-message --pane 12 --pane 15 --json
  cornercase read --pane 12 --pending --json
  cornercase read --pending --pane 12 --pane 15 --json
  cornercase read --messages --pane 12 --last 5 --json
  cornercase read --messages --pane 12 --before 41
  cornercase read --messages --pane 12 --after 42 --json";
