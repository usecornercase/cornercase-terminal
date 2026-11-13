pub(in crate::cli) const TUNNELS_HELP: &str =
    "Lists the tunnels held by the windows attached to this server: ports of this server's machine that
a window opened with `cornercase remote` forwards to its own computer. Each line gives the port,
the port it listens on there (`5173 → localhost:5174`) and its state: opening, up, or down with
the reason ssh gave, or closed: a tunnel that was closed stays listed so the window remembers it
(`tunnel forget` removes it). A window holds its tunnels as separate ssh processes, so a script on
the server's machine lists and drives them with this command and `tunnel open`, `tunnel close`
and `tunnel forget`. With several windows attached, each line says which one holds it, and a
tunnel held by `tunnel hold` ends with (hold N). A reverse tunnel, held by `tunnel hold --reverse`,
reads `9223 ← localhost:9222  up (reverse hold N)`: port 9223 of this server's machine reaches
port 9222 of the computer that holds it. With --json it prints
{\"tunnels\": [{\"port\", \"local\", \"state\", \"error\", \"holder\", \"kind\", \"direction\"}], \"windows\": N,
\"takers\": N}: local and error are null when they do not apply, holder is the number of the
holder's connection, kind is window or hold, direction is \"reverse\" for a reverse tunnel and left
out for the others, windows is how many windows hold tunnels for this server, 0 when none is
attached from another computer, and takers is how many programs take tunnel orders
(`tunnel orders`). `status --json` has the same entries, and under each of its `machines` the
tunnels that machine lists.

Examples:
  cornercase tunnels
  cornercase tunnels --json
  cornercase --remote vps tunnels";

pub(in crate::cli) const TUNNEL_HELP: &str =
    "A tunnel forwards a port of this server's machine to the computer of a window attached to it:
the window runs your own ssh, `ssh -N -L 127.0.0.1:LOCAL:localhost:PORT DESTINATION`, with the
destination it used, so every ssh setting of yours applies, and the far end is always the
machine's own loopback. The window listens on the same port when it is free there and on another
when it is not, and says which. `tunnel open PORT` asks every attached window, and every
program that takes tunnel orders, and waits up to 45 s until it is up; it prints one line per
holder and exits with 1 when none came up, or when neither is attached (tunnels are held by
windows opened with `cornercase remote`, or by programs that run `tunnel orders`).
`tunnel close PORT` closes it but keeps it remembered, as closed. `tunnel forget PORT` removes it.
A window remembers its tunnels per machine on its own computer, in tunnels.json beside
machines.json: the next window to that machine brings back the ones that are not closed, on the
same port when it is free. Windows of one computer share a tunnel: one runs the ssh and the others
take it over when it ends. With --json they print {\"tunnels\": [...], \"windows\": N, \"takers\": N}, as
`cornercase tunnels --json` does.

`cornercase --remote vps tunnel hold PORT` is for a program with no window: it starts its own ssh
from this computer, waits until it is up and prints one line, {\"port\", \"local\", \"url\"}, then
holds the tunnel until it is killed (exit 0), its output reader leaves (0), `tunnel close PORT` or
`tunnel forget PORT` is run on the server (0, after one more line: {\"ended\": \"closed\"} or
{\"ended\": \"forgotten\"}), the server stops or restarts (7, after {\"ended\": \"server-stopped\"} or
{\"ended\": \"server-restarted\"}: hold it again once the server is back), or ssh or the server
connection fails (1). It
needs --remote (exit 2 without it), and it remembers nothing in tunnels.json. If it fails
before the server counts it (ssh, or the local port), it tells the server, so a `tunnel open`
that waits for it fails with the reason. With --local N it listens on N when PORT is taken on this
computer (such as the port the hold had before, since it remembers nothing), and on any free port
when N is taken too; the line says which.

`cornercase --remote vps tunnel hold --reverse 9222` is the other direction: it makes port 9222 of
this computer reachable on 127.0.0.1 of vps, with `ssh -N -R 127.0.0.1:THERE:localhost:9222 vps`,
so a program on vps reaches, say, a browser's DevTools port here. THERE is a free port vps's ssh
server picks, or the one of `--there PORT`, which fails at once when PORT is taken there. Once it
is up it prints {\"port\", \"local\", \"url\", \"direction\": \"reverse\"}, where port is the port on vps
and url is http://127.0.0.1:PORT, and it ends as a hold does, with the same last line: `tunnel close
--reverse PORT` or `tunnel forget --reverse PORT` run on vps (0), the server stopping or restarting
(7), killed (0), or ssh fails (1). It listens only on
vps's loopback, which every program and user of vps can reach, and nothing on vps can open one.
`tunnel close PORT` and `tunnel forget PORT` without --reverse never end it.

`cornercase --remote vps tunnel orders` is for such a program too: it prints one JSON line once
the server counts it, {\"tunnels\": [...], \"windows\": N, \"takers\": N}, then one line per order,
{\"order\": \"open\", \"port\": 3000} (or close, or forget), for as long as it runs. `tunnel open` on
the server then asks it and waits: it answers by holding the port with `tunnel hold PORT`, or
with `tunnel failed PORT REASON`, which fails the request with REASON; `tunnel failed` prints
`reported that port 3000 could not be tunnelled` (with --json, that port's list) and exits 0 even
when no request waits. close and forget tell it which ports to drop or forget. It ends with 0
when its output reader leaves or the server stops, and 1 when the connection is lost; it needs
--remote (exit 2 without it).

Examples:
  cornercase tunnel open 5173
  cornercase tunnel close 5173
  cornercase tunnel forget 5173
  cornercase --remote vps tunnel open 5173 --json
  cornercase --remote vps tunnel hold 5173
  cornercase --remote vps tunnel hold 5173 --local 5174
  cornercase --remote vps tunnel hold --reverse 9222
  cornercase --remote vps tunnel hold --reverse 9222 --there 9223
  cornercase tunnel close --reverse 9223
  cornercase --remote vps tunnel orders
  cornercase --remote vps tunnel failed 3000 \"the user said no\"";
