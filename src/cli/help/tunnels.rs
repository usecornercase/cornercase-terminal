pub(in crate::cli) const TUNNELS_HELP: &str =
    "Lists the tunnels held by the windows attached to this server: ports of this server's machine that
a window opened with `cornercase remote` forwards to its own computer. Each line gives the port,
the port it listens on there (`5173 → localhost:5174`) and its state: opening, up, or down with
the reason ssh gave, or closed: a tunnel that was closed stays listed so the window remembers it
(`tunnel forget` removes it). A window holds its tunnels as separate ssh processes, so a script on
the server's machine lists and drives them with this command and `tunnel open`, `tunnel close`
and `tunnel forget`. With several windows attached, each line says which one holds it, and a
tunnel held by `tunnel hold` ends with (hold N). With --json it prints
{\"tunnels\": [{\"port\", \"local\", \"state\", \"error\", \"holder\", \"kind\"}], \"windows\": N}: local and
error are null when they do not apply, holder is the number of the holder's connection, kind is
window or hold, and windows is how many windows hold tunnels for this server, 0 when none is
attached from another computer.

Examples:
  cornercase tunnels
  cornercase tunnels --json
  cornercase --remote vps tunnels";

pub(in crate::cli) const TUNNEL_HELP: &str =
    "A tunnel forwards a port of this server's machine to the computer of a window attached to it:
the window runs your own ssh, `ssh -N -L 127.0.0.1:LOCAL:localhost:PORT DESTINATION`, with the
destination it used, so every ssh setting of yours applies, and the far end is always the
machine's own loopback. The window listens on the same port when it is free there and on another
when it is not, and says which. `tunnel open PORT` asks every attached window and waits up to
45 s until it is up; it prints one line per window and exits with 1 when none came up, or when
no window is attached (tunnels are held by windows opened with `cornercase remote`).
`tunnel close PORT` closes it but keeps it remembered, as closed. `tunnel forget PORT` removes it.
A window remembers its tunnels per machine on its own computer, in tunnels.json beside
machines.json: the next window to that machine brings back the ones that are not closed, on the
same port when it is free. Windows of one computer share a tunnel: one runs the ssh and the others
take it over when it ends. With --json they print {\"tunnels\": [...], \"windows\": N}, as
`cornercase tunnels --json` does.

`cornercase --remote vps tunnel hold PORT` is for a program with no window: it starts its own ssh
from this computer, waits until it is up and prints one line, {\"port\", \"local\", \"url\"}, then
holds the tunnel until it is killed (exit 0), its output reader leaves (0), `tunnel close PORT` or
`tunnel forget PORT` is run on the server (0), or ssh or the server connection fails (1). It
needs --remote (exit 2 without it), and it remembers nothing in tunnels.json.

Examples:
  cornercase tunnel open 5173
  cornercase tunnel close 5173
  cornercase tunnel forget 5173
  cornercase --remote vps tunnel open 5173 --json
  cornercase --remote vps tunnel hold 5173";
