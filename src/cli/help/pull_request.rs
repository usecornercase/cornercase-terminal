pub(in crate::cli) const PULL_REQUEST_HELP: &str =
    "Prints the pull request (the merge request on GitLab) of a workspace as its dialog shows it (`g p`,
or a click on the row's #482 tag): its state, its rollup and every check, failed first, then
pending, passed and skipped, each with its workflow (a GitLab job's stage) and the link to its job.

It asks gh (or glab) at once, as opening the dialog does, even while cornercase waits after a
failed read, and answers when that workspace was read. A read that fails exits with 1 and says
why (`gh is not logged in: run gh auth login`, `install gh, GitHub's CLI, to see them`, gh's own
line). A workspace cornercase never asks about is answered at once, with exit 0, saying why: pull
requests are off in settings, it is not on a branch, its repository has no remote, or it is on
the default branch. So is a branch without a pull request.

Without --workspace or --worktree it reads the workspace of the pane it runs in, or else the one
shown.

With --json it prints {\"workspace\", \"branch\", \"pull_request\", \"reason\"}: pull_request is the object
`cornercase status --json` gives (number, title, url, state, checks, failed, conflicts, base,
provider, stack) plus all_checks, a list of {name, workflow, outcome, url} where outcome is
failed, pending, passed or skipped, workflow is empty for a commit status and url is empty for a
check without a link. pull_request is null, with a reason, when there is none.

Examples:
  cornercase pull-request
  cornercase pull-request --worktree feat/login --json
  cornercase pull-request --workspace 4";
