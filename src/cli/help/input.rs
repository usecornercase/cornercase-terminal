pub(in crate::cli) const INPUT_HELP: &str = "For programs that draw cornercase's terminals themselves (see `cornercase surface`). Each
EVENT is one JSON object, sent in order:
  {\"key\": {\"code\": \"c\", \"mods\": [\"ctrl\"]}}   code as for `keys`; mods ctrl, alt, shift, super; kind press, repeat, release
  {\"text\": \"héllo\"}                           typed text, without control characters
  {\"paste\": \"two\\nlines\"}                     bracketed when the program asked for it
  {\"mouse\": {\"kind\": \"down\", \"button\": \"left\", \"x\": 3, \"y\": 1}}
                                             kind down, up, drag, moved, scroll-up, scroll-down,
                                             scroll-left, scroll-right; x and y are cells of the
                                             pane's terminal, from 0
- reads one event per line from standard input.

Events are encoded the way the program in the pane asked for (its keyboard protocol, application
cursor keys, bracketed paste, mouse mode), whatever the window shows: a dialog or a field there
takes nothing. Keys, text and pastes bring the pane's view back to the bottom. Mouse events and key
releases that the program does not take are not sent, and said on stderr. When one event is wrong,
or the program is not reading its input, nothing is sent. Scripts and agents should use `send` and
`keys`, which check before typing into an agent. With --popup the events go to the plugin popup open
in the window, its x and y being cells of the popup's terminal; add --pane with the popup's id from its
surface lines (popup.pane) so that nothing is sent if that popup closed and another one opened.

--boot BOOT says which run of the server the pane id (with --popup, the popup's id) is from: `boot` in
`status --json`, or in a machine's entry there. Ids last one run, so a server that restarted since
refuses, nothing is typed, and the command prints why and exits with 4. With --follow it goes with every
line: when the server restarted since, the first line ends the command with 4, and nothing after it is sent.

With --follow it keeps running and reads one event per line from standard input (give - as the
only EVENT) until it ends: each line goes as soon as it arrives, and the lines that arrive while
the answer before is on its way go together, so a program types through one process (and with
--remote one ssh connection). A line may name its pane, {\"pane\": 14, \"key\": {\"code\": \"enter\"}},
else it goes to --pane. Each request is all or nothing on its own: with --json it prints one line
each, the answer with events (how many lines it answers) or its error, and a line that is wrong or
refused (except one refused for --boot) does not stop the next ones; the command then exits with 3.

Examples:
  cornercase input --pane 12 --follow --json -
  cornercase --remote vps input --pane 9 --follow --json - --boot 4242-1700000000
  cornercase input --pane 12 '{\"text\": \"ls\"}' '{\"key\": {\"code\": \"enter\"}}'
  cornercase input --pane 12 '{\"mouse\": {\"kind\": \"scroll-up\", \"x\": 10, \"y\": 5}}'
  cornercase input --popup --pane 40 '{\"key\": {\"code\": \"down\"}}' '{\"key\": {\"code\": \"enter\"}}'
  cornercase --remote vps input --pane 9 '{\"paste\": \"two\\nlines\"}' --boot 4242-1700000000";
