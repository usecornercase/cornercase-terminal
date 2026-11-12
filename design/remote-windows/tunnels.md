# Remote windows › Tunnelling a port

Code: `remote/tunnel.rs`, `remote/tunnels.rs`, `client/window/tunnels.rs`, `client/window/tunnels/kept.rs`, `protocol.rs` (`Holding`, `TunnelOrder`), `server/windows.rs`, `app/tunnels.rs`, `app/ports_dialog.rs`, `ui/dialog/ports.rs`, `control/tunnels.rs`, `app/control/tunnels.rs`, `cli/tunnels.rs`, `cli/tunnels/hold.rs`, `app/machines.rs`, `ui/chrome.rs`, `test_util/program.rs` (#735, #766).

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
- Every link of the window (a reconnect, or a switch back) starts the tunnels that are down again, and brings back the remembered ones (below).
- Switching to another machine, or to this one, stops them all (`Tunnels::sync`) and then brings back what is remembered for the new destination; `Drop` of the handle stops ssh, so the window ending does too, while the memory stays. At most 32 tunnels per window, since a server can make the laptop start processes.
- A window killed with `SIGKILL` leaves its ssh until the connection drops. Linux's `PR_SET_PDEATHSIG` was left out: macOS has no equivalent and it would split the behaviour per OS.

## The protocol

`ClientMessage::Tunnels(Holding)` (index 9) carries the window's whole list, `browse` (whether it can open its default app) and each tunnel's port, local port and state (`Opening`, `Up`, `Down(reason)`, `Closed`); `ServerMessage::Tunnel(TunnelOrder)` (index 10) carries `Open(port)`, `Close(port)` or `Forget(port)`. `Closed` and `Forget` were appended last (#766), all are last in their enums, `VERSION` is 13, and an older peer reads them as `InvalidData` (`protocol/tests.rs`). A remote window sends its list after every link, order and change; a local window and the browser never send it, and the server drops a browser's. The proxy copies the bytes, so it needed no change. Orders go to windows only, never to the connections of commands.

The window opens a browser only for a port it tunnels: `Browse` for any other port, or for a LAN address, is refused with `port N is not tunnelled to this computer`, mapped to its local port otherwise. A remote window still sends no openers in `Hello`, so a server on the other machine never offers it ` open ` for the paths of its files.

## The server's side

`App` keeps each window's last report (`Holders`), with how many it sent, and which tunnels were asked by a click or by a request (`Origin`). The screen is drawn once for every client, so the dialog shows the tunnels of the window in use (`opening.machines.client`, the window that sent input last), as the openers do. A click orders its own window; `tunnel-open` orders every attached window (holds, below, are not windows: they get no orders and `windows` counts only windows, so `tunnel-open` with only a hold is still `unavailable`); a close or a forget orders every holder that lists the port, since the tunnel belongs to the destination, not to a window. A hold's "order" is the end of its stream.

## The dialog and the toasts

In the Ports dialog of a remote window, a program listening on every address, `127.0.0.1` or `::1` gets ` tunnel ` (not tunnelled), or ` close tunnel ` and the mark `⇄ …` (opening) or `⇄ :LOCAL` (up), with ` open ` (the window can browse) and ` copy ` of `http://localhost:LOCAL`. A LAN-only listener gets none: the far end is always `localhost`. A local window's dialog is unchanged.

A click that comes up says `port 5173 is tunnelled to localhost:5174 (5173 is taken here)` with ` open `; a request that comes up says `a program tunnelled port 5173 to localhost:5174` with ` close `; a failure always shows `could not tunnel port 5173: <ssh's line>`, so a tunnel that cannot come up is never silent. A tunnel that drops after it was up shows nothing: it is retried, and `cornercase tunnels` shows it down meanwhile.

## Requests

`tunnels`, `tunnel-open`, `tunnel-close`, `tunnel-forget` and `tunnel-hold` (see Commands for scripts and agents › Tunnels). `tunnel-open` answers once every window asked reported (the window reports after every order, changed or not, so the request never waits for a change that will not come), within 45 s (the window's 30 s plus connection slack); a close or a forget within 10 s (a close answers once the window reports the tunnel `closed`, a forget once it is no longer listed). `status`'s `tunnels` (left out when empty, so the v1 fixtures keep their bytes) and the `tunnel` event ride the traced step of the report, so `watch` and `status --follow` send a new snapshot.

## Remembered per destination

- **`tunnels.json`**, beside `machines.json` in the window's state folder (`paths::tunnels`): `{"vps":[{"port":5173,"local":5174,"pid":4242},{"port":3000,"local":3000,"closed":true}]}`. Compact, private (0o600) and written with `persist::write`; `closed` is left out when false, `local` and `pid` when unknown. It is per destination and per computer: it belongs to the window, not to the server (the server's machine does not know it).
- It is reread whole before every write and written only when something changed, with no lock, as `machines.json` is (`remote/tunnels.rs`). Each entry is parsed on its own, so a broken one, port 0 or a repeated port is skipped without losing the others; at most 64 per destination, sorted by port. Healing is the same as for `machines.json`: a lost write is repaired by the next write of that entry, and a link never removes a held port that the file lacks. A window that was not attached when another forgot a tunnel writes it back at its next own spawn; accepted, as for `machines.json`.
- **Close keeps the entry, forget removes it.** Close stops the ssh and leaves `closed: true` (state `Closed`, still listed by `tunnels`, `status` and the events); opening it again clears the flag. Forget stops it and removes the entry (an `Open` after that starts from nothing). The window brings back, at every link and switch, the entries that are not closed, on their remembered local port when it is free (`local_port`'s `before`). Rejected: removing on close (the issue wants the dev server's tunnels to come back, and closing one is not forgetting it).
- **The pid is written as soon as ssh is spawned**, not when it is up: a claim. Other windows then see a live process with the same forward that does not listen yet and wait (`Opening`) instead of starting a second ssh, which narrows the race that having no lock leaves.
- Forgetting a machine (`forget…`, `remote --forget`) leaves its tunnels in the file; they come back if the machine is reached again. Not built: it would need `Window::order(Forget)` and `cli/known.rs` to call a `forget_destination`.

## Shared by the windows of one computer

- Each entry's **owner** is the pid it records (`tunnel::owner`): it counts only when it is a live process (`process::running`) whose arguments hold the consecutive pair `-L 127.0.0.1:LOCAL:localhost:PORT` and end with `-- DESTINATION` (`process::args`: `/proc`, `KERN_PROCARGS2`, the PEB on Windows). The line is not compared whole, so options or a program in front still match (the e2e forwarder). `Listening` (it listens on LOCAL) is `Up`; `Starting` is `Opening`; `Gone` (dead, another program, another forward, or no `args` on this system) means the window opens its own, as before.
- A window whose entry has an owner other than itself starts no ssh: it shows the tunnel `Up` or `Opening` with the recorded local port and looks at that pid again every second (`CHECK_EVERY`). When it is gone the window **takes the tunnel over**: it rereads the entry (forgotten: the port is dropped; closed: it stays `Closed`; another window's new pid already listening: it watches that one) and otherwise starts its own ssh on the same local port and rewrites the pid, within about a second of noticing. The issue asked for 5 s.
- Ports held elsewhere cost no process, so they do not count toward the cap of 32. A tunnel closed or forgotten in the window in use is ordered in every holder (the server's close/forget orders all that list the port), so the window that runs the ssh stops it.
- **Converging after the fact, with no lock.** `tunnel open` orders every attached window in the same flush, so two windows of one computer usually both read the file before either claims, and both spawn. Both claims are written at spawn, long before either ssh is up, so when each comes up it reads the entry again (`converge`): if it names another pid that listens for the same destination, the window drops its own ssh and follows that one (`elsewhere`); otherwise it writes its claim again. Both read the same last claimer, so one ssh is left. A window whose ssh ends before it was up while the entry still holds its own pid (it lost the bind, and its claim was the last write) stays `Opening` and looks again after a second (`settling`), when the winner has rewritten the entry, instead of going down. A window that comes up in the instant between another's read and write can still leave two; accepted. Two windows noticing a dead owner within the same second can do the same.
- A tunnel adopted from an owner that listens counts as having been up (`was_up`), so a take-over that fails before it comes up is retried with `remote::backoff` like a tunnel of the window's own.
- `run_window` runs `tick` after an event when a timer is due (`wait` is zero): `tick` only ran when `recv_timeout` timed out, so a busy screen could starve tunnel retries and this take-over.

## The machine button and menu

- The button reads `vps ⇄2 ▴` while the window in use holds tunnels that are not closed (opening, up or down: what it tries to hold); with none it is as it was. The name is cut at its end, never the count: a row too narrow for one letter, `…`, the count and the mark falls back to the form without the count, and is left out only below the old minimum (8 cells). `View.machine_tunnels` and `ui::machine_button`'s third argument carry it.
- The machine menu lists the window in use's tunnels after its last row (`⇄ 5173 → localhost:5174  up`, `⇄ 3000  closed`, by port); holds are never listed there. A click opens a submenu at the same place, as `forget…` does: `open in browser` (up, and only when the window can browse), `close` (up, opening, down), `open` (down, closed) and `forget` (all). Each is `MachineItem::Tunnel(port)` and `TunnelDo(port, verb)`: a port is stable, so no index is kept. Close and forget go to every holder and toast (`closed the tunnel of port N`, `forgot the tunnel of port N`); open orders the window in use as the Ports dialog does.
- The Ports dialog shows ` tunnel ` again for a closed tunnel, like a down one.

## Holding a tunnel without a window

`cornercase --remote vps tunnel hold PORT` is for a UI with no window of its own (a Tauri or GPUI app, a script): it holds a tunnel for as long as it runs.

- It runs on the computer that should listen, starts its own `ssh -N -L` (`remote::tunnel::spawn`, the same line) and waits until it is up, then prints one compact JSON line, `{"port":5173,"local":5174,"url":"http://localhost:5174"}`, and registers with the `tunnel-hold` request. The request is the stream: the server lists the connection as a holder of kind `hold` (`kind` is `window` or `hold` in `tunnels --json` and the events, always written in the list, only `hold` in events), and `tunnel close` or `tunnel forget` ends its stream with `Detached`. The check of the server's feature (`tunnel-hold`, from the contract) shares the ssh connection with the request.
- The order matters: the line is printed before the request, so a program that only wants the URL has it as soon as the tunnel works; `--remote` is required (exit 2 without it, and inside a line of `cornercase requests`, which keeps no connection of its own).
- Exit codes: 0 when killed (SIGTERM, SIGINT, SIGHUP; Ctrl+C or Ctrl+Break on Windows, through `on_end_signal`), when its stdout reader leaves (not noticed on Windows, as with every follow), when it is closed or forgotten, or when the server stops or restarts (the stream's end); 1 when ssh fails before or after it was up (it is not retried: the program that holds it decides), or the server connection is lost; 2 for wrong usage.
- It reads and writes no `tunnels.json`: the program decides what to remember, and a hold killed with `SIGKILL` must not come back in a window. Its local port comes from `local_port(port, None)`, so it may differ from a window's for the same port. The request connection's ssh ends with the process, so the server forgets the hold even after `SIGKILL`; the tunnel's own ssh is left, the same limit as a window's.
- A hold never gets a `TunnelOrder`: `end_tunnel` branches on the kind. The server counts it in the list but not in `windows`.

## Tests

`remote/tunnel/tests.rs` (the line, the port choice including a wildcard listener, a fake `ssh` that listens or fails: `test_util/program.rs`'s `.forward`), `remote/tunnels/tests.rs` (the file), `client/window/tunnels/tests.rs` (the state with an injected spawner and owner, and a temporary file), `cli/tunnels/hold/tests.rs`, `app/tests/machines.rs` (the button's count, the menu, its submenus), `app/tests/ports_dialog/tunnels.rs`, `app/tests/control_requests/tunnels.rs`, `server/tests/tunnels.rs`, the UI snapshots, and `tests/e2e/tunnels.rs` (two windows sharing and taking over, a new window bringing tunnels back, `tunnel hold`), whose fake `ssh` runs a forwarder for `-N`. #735 was checked live on Linux against OpenSSH 10.2 (`ssh localhost`) on 2026-11-12. #766's remembering, sharing and take-over (two windows, the first one killed) were checked live the same way on that day, and `tunnel hold` only through the fake `ssh`; none of it on macOS or Windows.
