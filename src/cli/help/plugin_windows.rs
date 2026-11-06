pub(in crate::cli) const PLUGIN_WINDOWS_HELP: &str =
    "For a program that draws cornercase itself. Keep this command running for as long as your window is
open, and what each plugin that is on declares as [[window]] runs beside it, as beside a window of
cornercase: it starts what is wanted, reloads within a second when plugins change, starts again what
stops (after 1, 2, 4… up to 60 s) and keeps the files `plugin remove` reads. Two programs run two
copies, as two windows do. It reads the plugins of this machine, not a server: with no server, or no
plugin that has a window process, it keeps running and waits for one. An update does not restart it:
it stays the old version until you start it again.

It stops them all and exits with 0 on SIGTERM, SIGINT, SIGHUP or SIGQUIT (on Windows Ctrl+C, taskkill
without /f or its console closing), and when the reader of its standard output leaves: read it
through a pipe, and closing that pipe, or your program dying, ends it. Rust's Child::kill sends
SIGKILL, which nothing can catch: what it runs then keeps running on Linux and macOS, so close the
pipe instead. On Windows a process killed that way takes them along, but a reader that left is seen
only at the next line.

It prints a line for each change, such as `acme.notes panel: running, pid 4242`, `acme.notes panel:
stopped, restarts in 2s`, or `acme.notes panel: stopped` once it will not start again (the plugin
was turned off or lost that entry). With --json each line is a JSON object: plugin, id, state
(running or stopped), pid and restart_in (the seconds until it starts again, null once it will
not). Its output ends once everything has stopped. `plugin logs ID` says why one stopped.

It takes no --remote, and cannot run on a line of `requests`: window processes run on the machine
that shows the window.

Examples:
  cornercase plugin windows
  cornercase plugin windows --json
  cornercase plugin windows --json | your-window";
