pub(in crate::cli) const PORTS_HELP: &str =
    "Lists the TCP ports in LISTEN state held by the programs of a workspace: the processes its panes
started (and their children), and, in a workspace with a worktree of its own, any of your processes
working in that worktree. Each line gives the URL to open (http://localhost:PORT when it listens on
every address or on 127.0.0.1), the addresses, the program and its pid, and how it belongs to the
workspace: pane N, agent claude in pane N (the agent itself or a program it started), or cwd. A
plain workspace (no worktree of its own) shares its folder with the others, so only its panes count.
--all lists every port your processes listen on, those of no workspace too. Ports published by
Docker are held by Docker, not by the workspace that started the containers, so they show only
with --all. With --json it prints {\"ports\": [...]}: port, addresses, url, pid, program, args,
project, workspace, pane, via (pane, agent or cwd) and agent, null when they do not apply.

The text shows the program and its arguments cut to 40 characters; --json gives them whole.
`cornercase ports stop PORT` stops one (see its --help).

Examples:
  cornercase ports
  cornercase ports --worktree feat/login --json
  cornercase ports --all
  cornercase ports stop 5173";

pub(in crate::cli) const PORTS_STOP_ABOUT: &str = if cfg!(windows) {
    "Stop the program of a workspace that listens on a port: close its window, then end it after 3 s"
} else {
    "Stop the program of a workspace that listens on a port: SIGTERM, then SIGKILL after 3 s"
};

#[cfg(not(windows))]
pub(in crate::cli) const PORTS_STOP_HELP: &str =
    "Stops the program that listens on PORT, if it belongs to a workspace (`cornercase ports --all`
lists the others, which it refuses). It checks first that the program still holds the port, sends
it SIGTERM, waits up to 3 s for the port to be released, then sends SIGKILL. Only that process is
signalled, never its process group, so the shell or the agent that started it stays. When several
programs listen on the port, --pid says which. It prints stopped, killed (it ignored SIGTERM) or
still running, and exits with 1 when it is still running. With --json it prints {\"port\",
\"pid\", \"program\", \"outcome\"}, the outcome being stopped, killed or still-running.

Examples:
  cornercase ports stop 5173
  cornercase ports stop 3000 --pid 48211 --json";

#[cfg(windows)]
pub(in crate::cli) const PORTS_STOP_HELP: &str =
    "Stops the program that listens on PORT, if it belongs to a workspace (`cornercase ports --all`
lists the others, which it refuses). It checks first that the program still holds the port, asks
its window to close, waits up to 3 s for the port to be released, then ends it with
TerminateProcess; a program with no window is ended at once. Only that process is stopped, never
the programs that started it, so the shell or the agent that started it stays. When several
programs listen on the port, --pid says which. It prints stopped, killed (it had no window to
close, or did not close) or still running; it exits with 1 while the program still runs. With
--json it prints {\"port\", \"pid\", \"program\", \"outcome\"}: the outcome is stopped, killed or
still-running.

Examples:
  cornercase ports stop 5173
  cornercase ports stop 3000 --pid 48211 --json";
