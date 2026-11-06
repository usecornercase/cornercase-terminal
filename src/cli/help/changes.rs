pub(in crate::cli) const CHANGES_FILE_HELP: &str = "Prints one file's diff as a unified diff, with tabs as four spaces.

With --json it prints {\"workspace\", \"mode\", \"path\", \"old_path\", \"status\", \"added\", \"removed\", \"fold\",
\"viewed\", \"staged\", \"digest\", \"styles\", \"hunks\"}. A hunk is {\"old_start\", \"new_start\", \"context\",
\"lines\"} and a line is {\"kind\", \"old\", \"new\", \"text\", \"raw\", \"emphasis\", \"syntax\"}: kind is context,
removed or added; old and new are its numbers, null where it is not on that side; text is what a
window draws, with tabs as four spaces; raw is the line as it is in the file, tabs included, left out
when it would be the same as text; emphasis is the words that changed, as [start, end) pairs in
characters of text, left out when there are none; syntax is runs of {\"style\", \"text\"} over text,
style being an index into styles, left out where the line is not highlighted.

To copy a line as the file has it, take its raw, or its text when it has none. A tab of raw is always
four characters of text, so a selection made in text maps back to raw.

Examples:
  cornercase changes file src/main.rs
  cornercase changes file src/main.rs --mode commits --json
  cornercase changes file Makefile --json | jq -r '.hunks[].lines[] | .raw // .text'";

pub(in crate::cli) const CHANGES_GAP_HELP: &str =
    "Prints the unchanged lines between a hunk and the one before it, each with its old and new numbers
and its text, with tabs as four spaces.

With --json it prints {\"workspace\", \"path\", \"hunk\", \"styles\", \"lines\"} and a line is {\"old\", \"new\",
\"text\", \"raw\", \"syntax\"}, as in `changes file --help`: text is what a window draws, raw the line as
it is in the file, left out when it would be the same as text.

Examples:
  cornercase changes gap src/main.rs --hunk 1
  cornercase changes gap src/main.rs --hunk 1 --json";
