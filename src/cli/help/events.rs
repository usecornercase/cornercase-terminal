pub(in crate::cli) const EVENTS_HELP: &str = "It keeps running and prints one line per event, as it happens:
  pane 5  claude  working → waiting   an agent changed state: working, shell (its turn is over but a
                                      background shell it started still runs), waiting, done, idle,
                                      or none once it left the pane
  pane 7  cargo  exited               the program in the foreground ended and the shell is back
  tab 3  opened in workspace 2        a project, workspace, tab or pane opened, or closed
With --notices, each notice about an agent is told too, as the window's toast and desktop notification
say it, with the same rules (a second's wait, once per change, never for a tab seen in the window or
in a view), and with --json its token, agent, status, place, message and panes:
  workspace 2  notice  claude needs you in shop › main  cc-3f2a9c-4
An agent on a linked machine tells its notices too, with the machine's name and without ids here:
  machine vps  notice  claude on vps needs you in shop › main  cc-3f2a9c-5
`cornercase go TOKEN` then does what a click on that notification does.
The server's other notices come with --notices too, as the window's toast says them, with a kind and
no token: notify (`cornercase notify`, naming the pane it ran in), queue-started, queue-failed,
queue-missed and queue-kept (a queued agent or routine run; a one-off entry that missed its time sends
none), setup-failed (only while nobody looks at its tab), flow-consent (a flow's member waits for your
consent) and flow-merged (a run's agents closed once its pull request merged, worded like the toast
without its ▶: cornercase agents reopen --workspace ID brings them back). With --json they have kind,
message, panes and the workspace's ids, which queue-failed and queue-missed leave out, and failed: true on
a flow-merged whose free of the builds (--free-builds) failed:
  notify  the release build is ready
  workspace 2  flow-merged  pull request merged in fix-login: 3 agents closed
With --host, a program's own requests to the terminal are told too, for programs that draw the panes:
  pane 7  clipboard  11 bytes         it wrote to the clipboard (OSC 52); the text itself only with --json
  pane 7  bell  3 times               it rang the bell: one line a second per pane, with how many rang
With --json, each line is a JSON object with the event, the ids, the old and new state and the time.

It ends when the server stops, once every pane given with --pane has closed, or soon after what reads
its output is gone (a closed pipe or socket, such as an ssh that ended), with status 0. Programs are looked
at every half second while something follows events, so one that ends sooner may not show. Start it
before what you want to follow, or read `cornercase status` once it runs, so nothing falls between.
A reader that falls far behind loses events, and a line says how many.
With --boot BOOT, a server that restarted since refuses it before the first line (status 4); one that restarts while it runs ends it as above.

Examples:
  cornercase events
  cornercase events --pane 12 --pane 14 --json
  cornercase events --host --json
  cornercase events --notices --json
  cornercase --remote vps events --pane 9 --json --boot 4242-1700000000";
