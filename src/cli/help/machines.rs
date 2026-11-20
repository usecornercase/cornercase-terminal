pub(in crate::cli) const MACHINES_HELP: &str =
    "Lists this computer and the other machines work can start on, as Settings › Machines shows them,
each with the state of its link: the server keeps one ssh connection to each machine's cornercase,
through your own ssh and keys, and starts the server there with `start-server` when none runs. It
never stops that server, not when the link drops nor when the machine is removed; a server stopped
there comes back with the link, so remove the machine first to stop it for good. A link that drops
is tried again after 1, 2, 4 and 8 s, then every 15 s, and every minute while a person has to act
there (older, cornercase not found, this computer, the same server as another machine); `retry`,
this computer waking up or changing network try them all at once. ssh runs with BatchMode and
forwards nothing, so connect once by hand to accept the host key.

The states are connecting, linked (with the version there), unreachable (since when, and why) and
older. With --json it prints {\"here\": {\"icon\", \"version\"}, \"machines\": [{\"name\", \"icon\",
\"destination\", \"folder\", \"shown\", \"command\", \"state\", \"version\", \"server_version\", \"seen\", \"retry_at\", \"error\",
\"updating\", \"problem\", \"sync_flows\"}]}: version is the cornercase installed there and server_version the one
its server runs, seen when the link was last up (UTC), retry_at when a link that fell tries next
(UTC; a time that has passed means that try is under way; left out while the link is up or
connecting, and while a person has to act there), error why it is down, and problem what is wrong
with an entry of config.json, which then gets no link (shown is written only when false: such an
entry gets no link either, command only when set, and sync_flows only when true: the machine gets your
flows, see `cornercase flow sync --help`). `hide` keeps a machine on the list and ends
its link, so nothing of it shows here and its server keeps running there; `show` links it again, and
`all` hides or shows every machine at once. Every subcommand prints the list the same way.

Examples:
  cornercase machines
  cornercase machines add vps miguel@vps.example.com --icon server
  cornercase machines edit vps --folder '~/src'
  cornercase machines edit vps --sync-flows
  cornercase machines retry vps
  cornercase machines hide vps
  cornercase machines show all
  cornercase machines remove vps";

pub(in crate::cli) const MACHINES_ADD_HELP: &str =
    "A name is 1 to 20 letters, digits, `.`, `_` and `-`, unique ignoring case. The destination is what
ssh takes: user@host, host or a Host of ~/.ssh/config, unique, with no spaces and not starting with
`-`. The folder is absolute or starts with ~/, and is the machine's: nothing is made there yet. A
refused value exits with 1 and says why, and config.json is left as it was. --command is what runs
cornercase there for a window opened there (`edit --command \"\"` clears it); --hidden adds a machine
whose work is not shown here: it gets no link, and the machine button still offers it. --sync-flows
sends your flows there while it is linked, and every flow you make, change or delete after
(--no-sync-flows stops it; the flows already there stay).";

pub(in crate::cli) const MACHINES_UPDATE_HELP: &str =
    "Runs `cornercase update` on a machine whose cornercase is older than this one, through ssh with no
terminal: it installs the release and restarts nothing, so the server there keeps running its version
until it restarts there, and the list says so. The command waits for the install, then the link is
tried again. It exits with 1 when the machine is not older, or when the install fails, with the
last line it printed there (Homebrew's command, for one).";
