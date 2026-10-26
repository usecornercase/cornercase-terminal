pub(in crate::cli) const FILES_HELP: &str =
    "`list` prints the entries of one or more folders, `read` a file, and `search` finds files by name or by text, \
all as the files panel does, so a program on another machine (--remote) sees what the panel shows without \
reading the disk itself.

A path is relative to the workspace (the one you are in, or the one --workspace or --worktree names), never to \
the folder the command runs in; an absolute path, or one starting with ~/ (the server's home folder), reaches any \
folder on the server's machine, with no workspace needed, but then git marks only appear inside the workspace.

`list` prints one line per entry after a header: folder, name, kind (file, folder or other: a device, a pipe, a \
socket or a link to nothing), status (git's mark: modified, added, deleted, renamed or untracked, and for a folder \
added when everything in it is new and modified when it holds changes), bytes and modified (UTC). A - stands for \
nothing and . for the workspace's root. What git ignores and .git are left out, hidden files are listed (`hidden` \
in the JSON), at most 10000 entries per folder (said on stderr, `more` in the JSON), and a folder that cannot be \
listed prints on stderr and, with --json, in its `error`; the command still prints the rest and exits with 3.

`read` prints a text file as it is, up to 8 MB. An image (told by its first bytes) up to 32 MB prints its \
format, size and bytes; with --json its `image` holds those and `data`, the file in base64, and its `language` the \
grammar a viewer highlights it with. What is binary, too large, a folder or gone says so (`kind` in the JSON: \
text, image, binary, too-large, folder or missing); a folder or a missing file exits with 1. Only regular \
files are read, so a link to a device answers binary.

`search` prints the best 200 file names that match QUERY, ranked, smart-cased, or with --text (the default) \
the lines of at most 1000 matches as path:line:text, as `grep -n` does. It looks in the workspace only.

--follow on `list` and `read` keeps running and prints one JSON line each time the folders or the file change, \
checked on the server every 2 s for folders and every second for a file: first as it is now, with `stream`, \
`revision` and `boot`; each line replaces the one before. It ends when the workspace goes or once what reads it \
is gone. A connection follows one at a time.

Examples:
  cornercase files list --workspace 3 --json
  cornercase files list src --workspace 3 --follow
  cornercase files list ~/code --json
  cornercase files read src/main.rs --workspace 3 --json
  cornercase files search parse --text --workspace 3 --json
  cornercase files search main.rs --names";
