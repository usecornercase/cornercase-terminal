# Remote windows › Tunnelling a port

Code: `remote/tunnel.rs`, `client/window/tunnels.rs`, `protocol.rs` (`Holding`, `TunnelOrder`), `server/windows.rs`, `app/tunnels.rs`, `app/ports_dialog.rs`, `ui/dialog/ports.rs`, `control/tunnels.rs`, `app/control/tunnels.rs`, `cli/tunnels.rs`, `test_util/program.rs` (#735).

## Why

A dev server in a pane of the machine a remote window reached listens on that machine's loopback, so the browser of the window's computer cannot open it. The Ports dialog's ` open ` used to copy the URL there. A tunnel forwards that port to the computer of the window, with the user's own `ssh`, so their keys, agent and `ProxyJump` apply and cornercase handles no credential.

## The ssh line

One process per tunnel, started by the window and never by the server:

`ssh -N -T -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o ConnectTimeout=10 -o BatchMode=yes -o ExitOnForwardFailure=yes -o ControlPath=none -L 127.0.0.1:LOCAL:localhost:PORT -- DESTINATION`

- `-N`: nothing runs on the machine, so no shell and no `sh -c`, and the line is the same on every OS (`ssh.exe` on Windows).
- `--` before the destination, as in the links, so a destination can never be read as an option. The destination is the one the window used.
- `BatchMode=yes` because a prompt would read the terminal the window holds in raw mode; `ExitOnForwardFailure=yes` so a taken local port ends ssh at once with its own message instead of a half-open tunnel.
- **`ControlPath=none`** is the one change to the issue's line. With the user's `ControlMaster auto` (which the docs recommend for `remote`), an `ssh -N -L` becomes a client of the master, which holds the forward: the listener is then not our process, killing ours does not close it, it outlives the window by `ControlPersist`, and "ssh's pid listens" cannot say it is up. A tunnel therefore makes its own connection (one more handshake); keys, agent and `ProxyJump` still apply. Rejected: `-O forward` through the master (our own `ControlMaster` handling) and accepting the orphans.
- The far end is always `localhost` of the machine. The server sends only a port number, never a command, a host or a URL, so a server cannot make the laptop reach anything but the loopback of the machine it is attached to. It does choose which of the laptop's loopback ports are probed: each `Open` connects to `127.0.0.1` and `[::1]` on that port to see whether it is free, and the `local` port reported back tells the server whether something listens there. A server can therefore map the laptop's loopback services one port at a time (each `Open` also starts an ssh, bounded by the cap of 32 running); accepted as the cost of "same port when free".
- A `LocalForward`, `RemoteForward` or `DynamicForward` for that host in the user's ssh config makes every tunnel fail with `ExitOnForwardFailure` (the window's own connection already holds that forward, and `ClearAllForwardings`, which the links use, would clear ours too). A documented limit; the toast shows ssh's own line.

## Same port or another

LOCAL is PORT when nothing there is on it, else the last port used for it, else one the OS hands out (`local_port`). "Taken" is either of: a connect to `127.0.0.1` or `[::1]` answers within 200 ms, or `127.0.0.1` cannot be bound. A bind alone is not enough: std sets `SO_REUSEADDR` on Unix, so on macOS a bind of `127.0.0.1:p` succeeds while a program listens on `*:p`, and ssh (also `SO_REUSEADDR`) would then take that program's IPv4 traffic, and `http://localhost:PORT` would open the wrong thing. The connect probe can appear in a local service's log; accepted.

## Up means ssh's pid listens

The tunnel is up once the pid of its own ssh listens on `127.0.0.1:LOCAL` (`ports::listen::listeners`, every 100 ms, on Linux, macOS and Windows). Rejected: a probe connection (each opens a channel on the machine, and an unused port fills stderr with `open failed`), and parsing `ssh -v` (brittle across versions and `ssh.exe`). After 30 s without it, ssh is stopped and the tunnel is down with the reason. ssh's last stderr line is the reason when it ends first (`remote::Stderr`).

## Retries and endings

- ssh ends before it was up: down, with the reason, not retried by itself (the same thing would happen again).
- ssh ends after it was up: retried with `remote::backoff` while the window is linked.
- Every link of the window (a reconnect, or a switch back) starts the tunnels that are down again.
- Switching to another machine, or to this one, stops them all (`Tunnels::sync`); `Drop` of the handle stops ssh, so the window ending does too. At most 32 tunnels per window, since a server can make the laptop start processes.
- A window killed with `SIGKILL` leaves its ssh until the connection drops. Linux's `PR_SET_PDEATHSIG` was left out: macOS has no equivalent and it would split the behaviour per OS.

## The protocol

`ClientMessage::Tunnels(Holding)` (index 9) carries the window's whole list, `browse` (whether it can open its default app) and each tunnel's port, local port and state (`Opening`, `Up`, `Down(reason)`); `ServerMessage::Tunnel(TunnelOrder)` (index 10) carries `Open(port)` or `Close(port)`. Both are last in their enums, `VERSION` is 12, and an older peer reads them as `InvalidData` (`protocol/tests.rs`). A remote window sends its list after every link, order and change; a local window and the browser never send it, and the server drops a browser's. The proxy copies the bytes, so it needed no change. Orders go to windows only, never to the connections of commands.

The window opens a browser only for a port it tunnels: `Browse` for any other port, or for a LAN address, is refused with `port N is not tunnelled to this computer`, mapped to its local port otherwise. A remote window still sends no openers in `Hello`, so a server on the other machine never offers it ` open ` for the paths of its files.

## The server's side

`App` keeps each window's last report (`Holders`), with how many it sent, and which tunnels were asked by a click or by a request (`Origin`). The screen is drawn once for every client, so the dialog shows the tunnels of the window in use (`opening.machines.client`, the window that sent input last), as the openers do. A click orders its own window; `tunnel-open` orders every attached window; a close orders every window that lists the port, since the tunnel belongs to the destination, not to a window.

## The dialog and the toasts

In the Ports dialog of a remote window, a program listening on every address, `127.0.0.1` or `::1` gets ` tunnel ` (not tunnelled), or ` close tunnel ` and the mark `⇄ …` (opening) or `⇄ :LOCAL` (up), with ` open ` (the window can browse) and ` copy ` of `http://localhost:LOCAL`. A LAN-only listener gets none: the far end is always `localhost`. A local window's dialog is unchanged.

A click that comes up says `port 5173 is tunnelled to localhost:5174 (5173 is taken here)` with ` open `; a request that comes up says `a program tunnelled port 5173 to localhost:5174` with ` close `; a failure always shows `could not tunnel port 5173: <ssh's line>`, so a tunnel that cannot come up is never silent. A tunnel that drops after it was up shows nothing: it is retried, and `cornercase tunnels` shows it down meanwhile.

## Requests

`tunnels`, `tunnel-open` and `tunnel-close` (see Commands for scripts and agents › Tunnels). `tunnel-open` answers once every window asked reported (the window reports after every order, changed or not, so the request never waits for a change that will not come), within 45 s (the window's 30 s plus connection slack); a close within 10 s. `status`'s `tunnels` (left out when empty, so the v1 fixtures keep their bytes) and the `tunnel` event ride the traced step of the report, so `watch` and `status --follow` send a new snapshot.

## Not built (follow-up)

Remembering tunnels per destination (`tunnels.json`, closed vs forgotten, the count on the machine button and the menu's lines, taking over a tunnel another window of the laptop holds) and `cornercase --remote vps tunnel hold PORT` for a UI with no window. Two windows on one laptop can each open their own tunnel until `tunnels.json` exists (the follow-up).

## Tests

`remote/tunnel/tests.rs` (the line, the port choice including a wildcard listener, a fake `ssh` that listens or fails: `test_util/program.rs`'s `.forward`), `client/window/tunnels/tests.rs` (the state with an injected spawner), `app/tests/ports_dialog/tunnels.rs`, `app/tests/control_requests/tunnels.rs`, `server/tests/tunnels.rs`, the UI snapshots, and `tests/e2e/tunnels.rs`, whose fake `ssh` runs a forwarder for `-N`. Checked live on Linux against OpenSSH 10.2 (`ssh localhost`) on 2026-11-12; not on macOS or Windows.
