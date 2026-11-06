pub(in crate::cli) const CONFIG_PAIR_HELP: &str =
    "Does what the window's dialog does, for a program that draws cornercase itself: for browsers what
opening More > remote... > open in your phone's browser does, for a chat what Enter on the phone row
in Settings > Remote does. It prints the address the dialog's QR code holds and the lines under it,
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

Examples:
  cornercase config set remote true
  cornercase config pair browsers --json
  printf '%s' \"$BOT_TOKEN\" | cornercase config secret telegram
  cornercase config pair telegram.chat --json
  cornercase --remote devbox config pair discord.chat";
