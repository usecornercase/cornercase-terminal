pub(in crate::cli) const WORKTREES_HELP: &str =
    "Commands for the worktrees cornercase makes for workspaces. `size` reports the disk a worktree
takes and how much of it is build and dependency folders; `free-builds` deletes those folders.

Examples:
  cornercase worktrees size --worktree feat/login
  cornercase worktrees free-builds --worktree feat/login --check";

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
