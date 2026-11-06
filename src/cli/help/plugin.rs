pub(in crate::cli) const PLUGIN_HELP: &str =
    "A plugin is a folder with a cornercase-plugin.toml that declares commands: actions in the \
row menus, hooks on events, services the server keeps running and processes each local window keeps running. Its \
commands talk back through these same commands.

Install one from GitHub, any https:// git address or a git folder, pinned to a commit; a repository can hold \
several plugins, each in its own folder (owner/repo/folder on GitHub, address//folder elsewhere): you see where it comes \
from, what it asks for and every command it runs before anything runs, and it is installed turned off. Link a \
folder of yours instead while you write one.

`--preview` fetches a plugin, or an update of one, and prints that same preview, then exits without asking or \
installing, also with no terminal; with `--json` (and an ID for `update`) it is one JSON object, for a program that \
shows it and asks itself. `--commit` then installs, or updates to, exactly the commit the preview printed: it fails \
and installs nothing once the source gives another commit, and unlike SOURCE@COMMIT, which pins the plugin there, \
later updates still follow its branch or tag.

A plugin runs as you and can do anything you can, including typing into every terminal. What it asks for is shown \
before you turn it on, not enforced. A running server picks up changes within a second.

`actions` lists what plugins add to the menus, for a kind of row (--context) or all of them, and `run` runs one on \
a project, workspace, tab or pane as its menu entry does: in the workspace's folder, with the ids of what you \
name, stopped after 5 minutes, its output in the plugin's log. Both ask the running server; `run` waits for the \
action and exits with 1 when it fails. An action on a plugin's button runs with no target, on what the window \
shows. A popup action waits until its popup closes, and needs a window attached; `popup-close` closes it. An action \
listed before the plugins changed is refused (pass the --revision `actions --json` printed, or none for the current \
one).

A program that draws cornercase itself passes `run` the --view ID of its `cornercase view`: a button's action then \
runs on what that view shows, in its workspace's folder. A popup action opens with no window attached too, sized from \
that view while none is, and following it; it stays open if the view goes, as when the last window leaves, so the \
program that comes back shows it (`status --app`, `surface --popup`) or closes it with `popup-close`.

`status` prints the plugins as Settings › Plugins shows them: on, off or cannot run, the label and the note, and \
what each service is doing (running, restarting or still starting), and why `plugins.json` cannot be read. It asks \
the running server. `list` still reads the files and works with no server.

`windows` keeps the plugins' window processes running until it ends, as a window of cornercase does, for a program \
that draws its own window. See `cornercase plugin windows --help`.

Examples:
  cornercase plugin install acme/notes
  cornercase plugin install acme/notes@v1.2.0
  cornercase plugin install acme/notes --preview --json
  cornercase plugin update acme.notes --preview
  cornercase plugin install acme/tools/plugins/notes
  cornercase plugin install https://git.example.com/acme/tools.git//plugins/notes
  cornercase plugin link ~/code/my-plugin
  cornercase plugin enable my-plugin
  cornercase plugin list
  cornercase plugin status
  cornercase plugin status --json
  cornercase plugin actions --context pane
  cornercase plugin run acme.notes open --pane 5
  cornercase plugin run acme.notes browse
  cornercase plugin run acme.notes browse --view 21
  cornercase plugin popup-close
  cornercase plugin windows --json
  cornercase plugin logs my-plugin --follow
  cornercase plugin disable my-plugin
  cornercase plugin update
  cornercase plugin remove notes";
