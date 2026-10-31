pub(in crate::cli) const REQUESTS_HELP: &str =
    "For programs that send many commands, such as a window that draws cornercase itself (see `cornercase \
view`): it keeps one connection, and with --remote one ssh connection, and runs each line of standard input as it \
arrives, in order, until standard input ends. A line is a command as the command line takes it, in JSON, with what \
it reads from - in stdin:
  {\"args\": [\"focus\", \"--tab\", \"6\", \"--view\", \"21\"]}
  {\"args\": [\"send\", \"--pane\", \"5\", \"-\"], \"stdin\": \"cargo test\"}
Other keys are refused, so a typo never runs a command without its text. Each line gets one answer line, in order: \
status (the exit status the command would have had), json (what it printed with --json) or text, stderr (its \
warnings) and error. A wrong or refused line does not stop the next ones. Streams (events, surface, view, \
changes list, every --follow), start-server, update, restart, kill-server and logs need a process of their own and are \
refused with 2, as is --remote on a line. A line that waits (wait, send --wait, start --wait) holds the lines after \
it until it is answered. It exits with 0 when every line had 0, 3 when some did not, 1 when the server stops or the \
connection drops (it never reconnects: ids change with a restart), and 0 once what reads its answers is gone.

Examples:
  cornercase requests --follow -
  cornercase --remote devbox requests --follow -
  printf '%s\\n' '{\"args\": [\"status\", \"--json\"]}' | cornercase requests --follow -";
