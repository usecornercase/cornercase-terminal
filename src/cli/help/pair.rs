pub(in crate::cli) const CONFIG_PAIR_HELP: &str =
    "Does what the window's dialog does, for a program that draws cornercase itself: for browsers what
opening More > remote... > open in your phone's browser does, for app what Enter on pair a phone app in
Settings > Remote does, for a chat what Enter on the phone row in Settings > Remote does. It prints the address the dialog's QR code holds and the lines under it,
and again whenever they change, such as Discord's advice and failures. With --json each is one line:
{\"url\", \"lines\"}, url null while there is none yet (Tailscale still starting). Draw the QR code from url.

It stays open while it waits. A browser that takes the code ends it with {\"paired\": true}, a chat
with {\"paired\": true, \"chat\": NAME}, and status 0. Once nothing can pair it ends with
{\"paired\": false, \"error\"} and status 1: remote access turned off, Tailscale not ready, the bot's
token removed or refused, or ten minutes gone by without a phone. Why it cannot start at all goes to
standard error, also with status 1: remote access off, Tailscale not installed, the port taken, no bot yet.

The code is dropped when the command ends, also when whoever reads its output goes. While the window's
dialog or another `config pair` holds the same key they share one code: it lives ten minutes from the
last one that came, and it is dropped when the last one goes. Nothing opens in the window; once paired
it says so as it does for its dialog. The address holds a code, so treat it as a secret: it is never
written to server.log. Discord has no code: its owner is asked for while this runs.

`app` pairs the cornercase phone app, on the same network or over Tailscale, once `phone_app` is on. Its
address is the app's own code: ws://HOST:PORT/?code=CODE&also=HOST&name=NAME, with every address the
computer listens on for the app (also= for each one after the first, IPv6 in brackets only in the first)
and the computer's name. The app opens it as a WebSocket, which trades the code for a secret of its own
and ends the command with {\"paired\": true}. --routes does not wait: it prints the addresses the app
can reach now, one HOST:PORT per line or, with --json, {\"routes\": [{\"host\", \"port\"}]}, so an app
that paired earlier can add a network that came up since. A paired app is forgotten with
`config remove app.ID`.

Examples:
  cornercase config set remote true
  cornercase config pair browsers --json
  cornercase config set phone_app true
  cornercase config pair app --json
  cornercase config pair app --routes --json
  printf '%s' \"$BOT_TOKEN\" | cornercase config secret telegram
  cornercase config pair telegram.chat --json
  cornercase --remote devbox config pair discord.chat";
