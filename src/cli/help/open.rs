pub(in crate::cli) const OPEN_HELP: &str =
    "PATH is a folder, or a VS Code .code-workspace file, which is read on the server's machine and
imported as the folder picker does: into a group named after the file (a group of that name is
reused), each of its folders a project there, in order. A folder already open moves into the group
instead of opening twice, a folder's `name` names a project that has no name of its own, and a
folder that is gone is skipped. So is one on another machine (a vscode-remote:// address, a network
path on Windows), as in the picker: it counts nowhere and changes no exit status.

A file prints the group's id, and with --json {\"group\", \"name\", \"projects\", \"missing\", \"failed\"}:
the group (null when nothing could go in a new one), its name, the projects' ids, how many folders
are gone and how many could not start a shell. It exits with 3 when either count is not 0 or
nothing was imported, after printing that; without --json, a warning on stderr says what happened.
--focus shows its first project.

--into GROUP puts what it opens in that group, as the group's add project does, a folder or a file.
A folder already open moves there.

With --remote, PATH is the other machine's, and so is the file: absolute, or starting with ~/,
quoted so that this machine's shell leaves it alone.

Examples:
  cornercase open ~/src/shop
  cornercase open . --focus
  cornercase open ~/src/team.code-workspace --focus
  cornercase open ~/src/docs --into 3
  cornercase --remote devbox open '~/src/team.code-workspace' --json";
