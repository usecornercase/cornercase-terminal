pub(in crate::cli) const WORKTREES_HELP: &str =
    "Commands for the worktrees cornercase makes for workspaces. `list` shows every worktree on disk
and what makes each safe to remove; `size` reports the disk a worktree takes and how much of it is
build and dependency folders; `free-builds` deletes those folders.

Examples:
  cornercase worktrees list
  cornercase worktrees size --worktree feat/login
  cornercase worktrees free-builds --worktree feat/login --check";

pub(in crate::cli) const WORKTREES_LIST_HELP: &str =
    "Lists every worktree on disk as the Worktrees dialog does, in its order: the leftovers first
(prunable, repository gone, unknown to git), then the merged ones, then the rest, the oldest first.
Each one shows its project and branch, folder, when it last changed, its size with the part in
build folders and, under it, what removing it would lose or stop: uncommitted files, commits not
pushed, a lock, programs running in it. It covers the worktrees of the open projects and those of
closed projects left in the worktrees folder; --project ID lists one project's. It removes
nothing. Measuring walks every worktree, two at a time, which takes a while with big trees;
--no-sizes answers without it. A worktree that could not be checked or measured says so, and the
command exits 3 after printing the rest. With --json it prints {\"worktrees\": [...]}, each with
\"path\", \"kind\" (linked, prunable, repository-gone, unknown-to-git), \"project_name\",
\"repository\", \"branch\", \"project\" and \"workspace\" (when open in cornercase),
\"last_changed\", \"changed\", \"unpushed\", \"merged\", \"no_commits\", \"upstream_gone\", \"lock\",
\"running\", \"outside\", \"other_tabs\", \"removing\", \"run_role\", \"safe\" (removing it loses no
uncommitted file and stops no program), \"risks\", \"error\", \"size\" and \"size_error\".

Examples:
  cornercase worktrees list
  cornercase worktrees list --project 2 --json
  cornercase worktrees list --no-sizes";

pub(in crate::cli) const WORKTREES_SIZE_HELP: &str =
    "Measures a worktree on disk: the bytes that removing it would free, and the part in build
folders, the git-ignored folders whose names are on the project's list (node_modules, target,
.next, .turbo, dist, build, .venv and __pycache__ unless `cornercase config project --builds`
changed it). Hard links count only when every link is inside the worktree, and symbolic links are
never followed. The target is a workspace with a worktree of its own (--workspace, --worktree
BRANCH or the one this runs in) or any linked worktree by --path, open or not. A walk of a big
tree takes a second or more. With --json it prints {\"path\", \"bytes\", \"builds_bytes\", \"builds\":
[{\"path\", \"bytes\"}], \"nested\": [...]}; nested lists git checkouts found inside build folders.

Examples:
  cornercase worktrees size
  cornercase worktrees size --worktree feat/login --json
  cornercase worktrees size --path ~/.cornercase/worktrees/shop/feat-login";

pub(in crate::cli) const WORKTREES_FREE_HELP: &str =
    "Deletes the build folders of a worktree: only folders git ignores whose names are on the
project's list, at any depth. The worktree, its branch and its changes outside those folders stay, and
the project's setup can rebuild what went; files inside the deleted folders are lost. --check prints what would go and the programs running in
the worktree, and deletes nothing. Without --force it refuses (exit 1, code `conflict`) while a
program runs in the worktree or a build folder holds a git checkout, since those may still use the
folders or lose changes; --force goes on and deletes a nested checkout's uncommitted changes too. Folders that could not be deleted, or that git stopped
ignoring since the measure, are named and the command exits 3. With --json it prints {\"path\",
\"check\", \"bytes\", \"folders\": [{\"path\", \"bytes\"}], \"kept\", \"failed\": [{\"path\", \"error\"}],
\"running\"}.

Examples:
  cornercase worktrees free-builds --worktree feat/login --check
  cornercase worktrees free-builds --worktree feat/login
  cornercase worktrees free-builds --path ~/.cornercase/worktrees/shop/feat-login --force";
