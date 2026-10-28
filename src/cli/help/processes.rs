pub(in crate::cli) const PROCESSES_HELP: &str =
    "Lists what every pane runs, as the Processes dialog does (More ▴ › processes): for each pane the
processes under its shell, each with its CPU and its memory, and the pane's sums. The last line
before the total is cornercase itself: the server and what it runs outside the panes (plugin
services, plugin popups, its own git jobs). CPU is the share of one core over --interval (100%
is one full core, as in top; default 1000 ms, from 100 to 10000), so the command takes that long.
Memory is the footprint of each process, the figure the tab rows show. --pane or --workspace lists
only the panes concerned, and then leaves cornercase's own line out. The text sorts the panes by
CPU, highest first; --json keeps the sidebar's order. With --json it prints {\"interval\", \"total\":
{cpu, memory, panes, processes}, \"panes\": [...], \"cornercase\": {...}}: each pane has pane, tab,
workspace, project, program, agent, foreground, pid (its shell), cpu, memory and processes, each
process pid, parent, name, args, started, cpu and memory (null when it cannot be read). The
started value is only to be given back to `processes stop --started`.

Short-lived processes that start and end between two samples are not counted, and a program that
daemonizes leaves its pane's tree. `cornercase processes stop PID` stops one (see its --help).

Examples:
  cornercase processes
  cornercase processes --pane 12 --json
  cornercase processes --workspace 3 --interval 2000
  cornercase processes stop 48211";

pub(in crate::cli) const PROCESSES_STOP_HELP: &str =
    "Stops a program of a pane and everything it started: PID and its descendants, found when the
command runs. It sends them SIGTERM, waits up to 3 s for them to end, then sends SIGKILL to
the ones still running. It never stops a pane's shell (close the pane instead: `cornercase close
--pane ID`), cornercase itself, or a process that belongs to no pane; each of those is refused
with a conflict. A pid that is not running is refused as not found. With --started (the value
`processes --json` gave), a pid that was reused by another program since is refused too. It
prints stopped, killed (they ignored SIGTERM) or still running, and exits with 1 when something
is still running. With --json it prints {\"pid\", \"program\", \"pane\", \"outcome\", \"processes\"}, the
outcome being stopped, killed or still-running and processes how many were signalled.

Examples:
  cornercase processes stop 48211
  cornercase processes stop 48211 --started 81240 --json";
