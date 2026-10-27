pub(in crate::cli) const PASTE_FILE_HELP: &str =
    "Keeps a file of the machine you run this on in the server's state folder and pastes its path into the \
pane, followed by a space (bracketed when the program asked for pastes so), so an agent there can open it. With \
--remote DESTINATION the file goes to that machine through ssh: a screenshot on your laptop becomes a path the \
agent on that machine can read.

FILE is read here, up to 32 MB; without it, or with -, standard input is, and an empty one is refused. The name \
there is --name, else FILE's name, cleaned to letters, digits and . _ + - so the path needs no quoting; a name \
without an extension gets an image's own (png, jpg, gif, webp, bmp, ico), told by its first bytes. Each file gets \
a folder of its own in pasted/, next to the server's session.json: it is removed when the pane closes, and what an \
earlier server left there after 7 days. Prints the path; --json prints the pane, the path and the file's bytes.

Like `input`, it pastes whatever the program in the pane is doing: a dialog or a question there gets the path too.

Examples:
  cornercase --remote devbox paste-file --pane 12 ~/Desktop/shot.png
  pngpaste - | cornercase --remote devbox paste-file --pane 12
  wl-paste --type image/png | cornercase --remote devbox paste-file --pane 12 --name error.png";
