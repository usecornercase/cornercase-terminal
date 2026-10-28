# Shared files and the merge check

Code: `changes/overlap.rs`, `changes/merge.rs`, `app/overlaps.rs`, `app/merge_check.rs`, `app/control/merge_check.rs`, `ui/workspaces.rs`, `ui/changes.rs`, `control/changes.rs`, `cli/changes.rs` (#252).

Two workspaces of one project are two git worktrees of one repository, and nothing stops two agents from changing the same file in them. The overlap only shows at merge time, so cornercase says so earlier: a `⇄n` badge on the rows, a `⇄` mark on the files in the changes panel, and a **merge check** that merges the work in a scratch copy and names the conflicts.

## What counts as shared

- **Pairwise divergence, not versus the base branch.** Two workspaces share a file when both changed it *since their two branches parted* (`git merge-base` of their `HEAD`s), committed, staged, unstaged or untracked. The issue worded it as each workspace against the base branch, and that was rejected: a stacked branch (see Pull requests, Stacks) or a worktree made from unpushed commits would share every file it inherited, and merging those never conflicts. The pair's own merge base is what `git merge` and `git merge-tree` use. The base branch comes in through the merge check ("merges into origin/main?"). To go back to the literal reading, change only `overlap::scan`.
- **`--no-renames`**, so a rename counts both paths and a rename against an edit of the old path is caught. Paths are workspace-relative (`--relative`), as the panel's `File.path` is; only a repository's root has several checkouts, so these are checkout-root paths.
- **Untracked files count** (`ls-files --others --exclude-standard`), like the panel's Uncommitted mode. An untracked file does not need a commit to collide.
- **Plain workspaces sharing one folder are one checkout.** They never pair with each other, and each pairs with the other checkouts as that checkout (the dialog may list two plain workspaces for the same folder).
- **Every project shows only uncommitted changes and has no base** (see Changes panel, Scope), so the overlap does not live in the panel's scope: it has its own scan, and the panel only reads its result for the `⇄` mark, in both scopes.
- The badge number is the distinct files shared with any other workspace, not the number of workspaces.

## The scan

- `App::scan_overlaps`, on the 1 s sync step, one job per project at a time (`Job` `changes`/`overlaps`), only in projects with two or more open git checkouts: a plain folder or a project with one checkout costs nothing. It never takes the changes panel's one worker slot (`Panel::loading`), so the panel never waits for it, and it never runs while drawing.
- **Paced from the end of a scan**: `SCAN_EVERY` (5 s) after the last answer, so a slow repository is never scanned back to back.
- **Cost** measured on a repository of 1,907 files (2026-10-27, git 2.43, Linux), with the panel's flags (`--no-optional-locks`, so never `index.lock`): `git diff --name-only -z --no-renames --relative <head> -- .` 0.09–0.15 s, `git ls-files --others --exclude-standard -z` 0.01–0.06 s, `git merge-base` 0.01 s, per checkout. The pairs' own work is cached by the heads of the two branches (`Known`): commits never change, so a steady state runs no process per pair, only the two per checkout.
- Reading is in memory only (`shared_with`, `shared_count`, `shared_files`): no git and no file reads while drawing, in `workspace_entry`, `panel_view` or the dialog's view, so the 2 ms diff view of the FAQ ("Is it fast?") is untouched. A closed or removing workspace stops counting at once, before any rescan. The FAQ and `Stats.astro` numbers are a dated snapshot and were not re-measured.
- **`shared_files`, off by default** (Settings › Worktrees, the user's decision after the feature shipped on): the badges and marks were noise for most projects, and the scan costs about 0.1–0.2 s of git per checkout every 5 s. Off, `scan_overlaps` drops every result and starts no job, and `shared_with` answers nothing even before the next tick, so the badge, the panel's mark and `status`'s `shared` go at once. The merge check stays in the row menu and on the socket: it is on demand, and with nothing shared it checks only the base. Its dialog then says shared files are off instead of "No other workspace … changes the same files", which would claim a scan that never ran; the socket's answer has no such note, and a script reads the setting with `config get shared_files`.

## The row, the panel and the menu

- **`⇄n`**: U+21C4, one cell wide (East Asian Width Neutral), in yellow (caution; orange 208 stays for real conflicts and waiting agents). Rejected: `⚠` (emoji width), `∩` (ambiguous width), a word (too wide). Order of a row's tags: attention badge, `▶`, `setting up…`, pull tag, stack, **`⇄n`**, `↓n`, so `Tags::fit` drops `↓n` first, then `⇄n`, then the stack and the pull tag. Hidden while the row is being removed.
- Pressing it opens the Merge check dialog for that workspace without selecting it, and closes the compact menu (like the other tags). A right-click on it still opens the row menu. Hover underlines it.
- The row menu offers **merge check** for every open git workspace, after the pull request entry, even with nothing shared, since the base check is useful on its own.
- In the changes panel a shared file's row has a yellow `⇄` as its first right-hand tag, muted like the counts when the row is muted. It has no click target. A rename counts when its new or old path is shared.

## The merge check

- `git merge-tree --write-tree --name-only -z --messages <ours> <theirs>` (git 2.38 or later) merges two commits without touching a checkout, an index or a branch; it exits 0 clean and 1 with conflicts, and `<conflict-type>` is "a stable string" in its manual. Older git gets the message "the merge check needs git 2.38 or later" in the dialog and in the answer's `error`. `--merge-base` (2.40) refused trees on 2.43 ("expected commit type"), so uncommitted work must become a commit.
- **The snapshot of uncommitted work**: a copy of the checkout's index (git replaces the index by rename, so a copy is never torn) in a scratch folder, `add -u` (every tracked change, deletions included, fast on the copied stat data, `core.splitIndex=false`), the untracked files up to the panel's own caps (1 MiB each, 8 MiB in all; the dialog says how many were left out), `write-tree`, then `commit-tree --no-gpg-sign -p HEAD` with a fixed identity in the environment, since test repositories and some users have no `user.name` and a signing prompt must never appear. Symlinks are kept.
- **Nothing in the repository changes**: the git runner drops the server's inherited repository variables (see Worktrees); then every command of a check runs with `GIT_OBJECT_DIRECTORY` at a scratch folder and `GIT_ALTERNATE_OBJECT_DIRECTORIES` at the repository's objects, and the snapshot with `GIT_INDEX_FILE` at the copy. The scratch folder (`cornercase-merge-<pid>-<n>` in the temp folder, one per check) is removed on drop. `changes::merge::tests::nothing_in_the_repository_changes` compares `git status`, both index files, the refs and `git count-objects -v` before and after, and fails without those variables.
- **Rejected**: `git stash create` (it can write the real index) and `git add -A` (no caps: an un-ignored `node_modules` would be hashed whole). A clean filter such as LFS still runs as in `git add`: a modified LFS file is copied into `.git/lfs/objects`; no worktree is touched.
- **What is merged**: the workspace's snapshot *into* its base (`Workspace::base`, else the panel's default: `origin/HEAD`, `main` or `master`) and with each workspace it shares files with. A base that does not exist is that check's error (`no branch X`); none at all is `no base branch to merge into`.
- **On demand only**: the dialog checks when it opens and on **check again**, the socket on request; no automatic dry runs, to keep the background cost at names-only scans. One job per check, 30 s in all (`GIVE_UP_AFTER`).
- **The dialog** is the `ui::Update` dialog, as the pull request's. Lines are never wrapped (labels with `truncate_cells`, paths with `truncate_left`); at most 20 files and 20 conflicts are listed, then `… and N more`. Closing during a check drops its answer: a generation counter, so a reopened dialog never shows an older check's result. It closes when its workspace closes or starts being removed. Conflict kinds come from git's `CONFLICT (kind)` records; a conflicted path no record names is `conflict`.

## Scripts

- `status` gives `WorkspaceInfo.shared`: per other workspace `workspace`, `count` and at most 100 `files` (`SHARED_LISTED`, so a formatter run does not bloat every `watch` snapshot), left out when empty so the v1 `watch` and `view` fixtures are unchanged; `App::refreshed` hashes it, so `watch` and `status --follow` send a line when it changes. Feature `shared-files`.
- `changes-merge-check` (`workspace`, `with`, `base`): see Commands for scripts and agents › Changes. `cornercase changes shared` reads `status` only, so it works against any server that answers it.

## Website simulation

The demo has the menu entry only, with a "needs cornercase on your computer" toast; its workspaces share no files, so no `⇄` is drawn.

## Rejected or left for later

Automatic dry runs in the background (a snapshot per tick), hunk-level overlap, the badge turning orange after a failed check, a key in the keys menu, **ask agent** from the dialog, clicking a pair to go to that workspace, overlap across projects, and flow role worktrees (they are not workspaces).
