pub(in crate::cli) const SETUP_HELP: &str =
    "Runs the project's setup again in a new tab of a worktree's workspace, as run setup in the
workspace's menu does, to retry after a fix. It runs as it does when the worktree is made, with
CORNERCASE_ROOT, CORNERCASE_WORKTREE and CORNERCASE_BRANCH set, and the tab then becomes a shell.
Meanwhile `status --json` shows the workspace's setup as running, its failed mark is gone, and
agents started in the workspace wait for it.

The command returns once the setup is done, however long it takes, and prints the pane's id; --json
prints {\"project\", \"workspace\", \"tab\", \"pane\"}. A setup that fails exits with 1, naming its exit
status and the pane to read, and so does one whose tab closes before it ended. It also exits with 1,
saying why, for what the menu does not offer: a workspace that is not a worktree, a project with no
setup (`cornercase config project`), or a setup already running. Stopping the command does not stop
the setup: press Ctrl+C in its tab (`cornercase keys --pane N ctrl+c`) or close the tab.

Examples:
  cornercase setup
  cornercase setup --worktree fix/login --focus
  cornercase setup --workspace 16 --json";
