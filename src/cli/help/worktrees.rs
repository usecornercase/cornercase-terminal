pub(in crate::cli) const WORKTREES_HELP: &str =
    "Commands for the worktrees cornercase makes for workspaces. `list` shows every worktree on disk
and what makes each safe to remove; `size` reports the disk a worktree takes and how much of it is
build and dependency folders; `free-builds` deletes those folders; `remove` removes what `list`
shows, as the Worktrees dialog does.

Examples:
  cornercase worktrees list
  cornercase worktrees size --worktree feat/login
  cornercase worktrees free-builds --worktree feat/login --check
  cornercase worktrees remove --path ~/.cornercase/worktrees/shop/feat-login --check";

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

pub(in crate::cli) const WORKTREES_REMOVE_HELP: &str =
    "Removes worktrees as the Worktrees dialog does, one or more, each named by its folder as
`worktrees list` prints it, open in cornercase or not: an open workspace stops its programs, runs
its project's teardown and goes, a closed project's worktree runs that teardown too, a prunable
entry leaves git's list, and a folder whose repository is gone or that git no longer knows is
deleted. Only a folder `worktrees list` knows is touched: anything else, a repository's main
checkout and a project's own folder included, is refused and left alone, as is a worktree git
records at a symbolic link, which git would delete through, and a folder holding another listed
worktree or an open project, which would go with it unseen (remove or close that one first). So is
what the dialog cannot tick:
a worktree of a role of a live flow run (it goes with the run's workspace), one being removed or
closed, one whose build folders are being freed, and an open workspace git no longer knows (close
it, then remove the folder).

--check prints what the dialog's confirmation names for each folder (files changed, commits not
pushed, a lock, the programs that stop, processes outside cornercase and in other tabs, the
teardown, with --delete-branch a branch that would lose commits), and what removing it takes. It
stops and deletes nothing. Without it the folders are checked again the same way, and one that is
at risk is refused: --force goes on when it has uncommitted files or could not be checked, when
programs work in it, when its files are all that is left (repository gone, unknown to git), when
it holds commits on no branch, or when --delete-branch would lose commits; --unlock is needed for a
locked one and does not imply --force. --delete-branch deletes the branch checked out there after
the worktree is gone, and a run's workspace its roles' branches too, never the default branch or
the main checkout's, and only the branch still checked out when the removal starts, at the commit
the check saw.

It waits until every removal has ended and prints one line per folder: removed, could not remove
or did not remove, and why. It exits 3 when some folder was not removed, its branch was kept or
could not be deleted, its teardown failed, or only git's entry went and its folder stays (with
--check, when some folder or role cannot be removed or checked), after printing the rest. A
command that goes away before the removals start leaves everything as it was; one that goes away
later leaves them running. With --json it prints {\"check\", \"folders\": [...]}, one per --path in
order, each with \"path\", \"outcome\" (removed, failed, refused; removable with --check), \"why\",
\"needs\" (force, unlock), \"removed_with\", \"branches\" (deleted), \"kept\" (branches kept and why),
\"warnings\", \"lines\", \"lost\", \"unchecked\", \"teardown\", \"roles\" and \"worktree\" (the row `worktrees
list` prints).

Examples:
  cornercase worktrees remove --path ~/.cornercase/worktrees/shop/feat-old --check
  cornercase worktrees remove --path ~/.cornercase/worktrees/shop/feat-old --delete-branch
  cornercase worktrees remove --path ~/.cornercase/worktrees/shop/a ~/.cornercase/worktrees/shop/b --force --unlock";
