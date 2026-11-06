pub(in crate::cli) const RECAP_HELP: &str =
    "Prints the Recap panel's summary of what your agents did: `now` (where things stand, what waits
on you first), `yesterday` (Done / In progress / On you, for the last day with agent activity, so
on Monday it is Friday) or `week` (per project, the last 7 days). An agent (Claude Code or Codex,
set in Settings > Agents > Recap) reads a dossier cornercase builds from the journal of your
agent conversations and writes it in the background; it runs without a pane, reads transcripts
and `cornercase search`, and changes no project. The text names each conversation as
`project > workspace > tab` and each pull request as `#number`.

A current result is printed at once (`now` for 30 minutes, `yesterday` once per day, `week` once
per day). Otherwise the command waits for the agent. --fresh writes it again. --agent, --account,
--model, --effort and --language change the settings for that run only, and write it again.
--dossier prints the dossier without running an agent.

Nothing to sum up prints why and exits 0. A run that fails prints the last good text, then
`recap unavailable:` and the reason, and exits with 3. A server that does not know `recap` is
refused by name.

With --json it prints {\"range\", \"day\", \"label\", \"from\", \"to\", \"zone\", \"written_at\", \"age\",
\"agent\", \"title\", \"account\", \"model\", \"effort\", \"language\", \"tokens\", \"conversations\", \"text\",
\"places\", \"dossier\", \"loading\", \"progress\", \"empty\", \"error\"}. text keeps the ids ([c3], [p1]);
places maps each id to {kind, project, workspace, branch, tab, agent, conversation, pane, boot,
number, url, state}. The pane a conversation is in now is the one in `cornercase status --json` with
its agent and conversation; a null conversation matches nothing, and then pane is worth trying
while boot is the server's. `cornercase events` reports a recap starting, finishing or failing.

Examples:
  cornercase recap
  cornercase recap yesterday --language Spanish
  cornercase recap week --fresh --json
  cornercase recap --dossier yesterday";
