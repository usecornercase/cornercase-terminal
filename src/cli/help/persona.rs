pub(in crate::cli) const PERSONA_HELP: &str =
    "A persona is how an agent works, written as Markdown: a front matter (a `---` block) that gives its
`name`, and often a `description`, an `emoji` and a `color`, then the instructions. A flow's role
names one with `persona = \"ID\"`, on any agent: when the run starts, cornercase copies it into the
run's folder (personas/ID.md) and the role's prompt tells the agent to read and follow it before its
first work. Yours live in personas/ next to config.json, one ID.md each, the ID taken from the file's name.

Examples:
  cornercase persona import msitarzewski/agency-agents/engineering/engineering-code-reviewer.md
  cornercase persona import msitarzewski/agency-agents/engineering
  cornercase persona import msitarzewski/agency-agents --check
  cornercase persona import ./reviewer.md
  cornercase persona list
  cornercase persona remove engineering-code-reviewer";

pub(in crate::cli) const PERSONA_IMPORT_HELP: &str =
    "SOURCE is a .md file, a folder, - for standard input, an https:// address, or a GitHub repository
as OWNER/REPO, OWNER/REPO/FOLDER or OWNER/REPO/PATH/FILE.md (read from its default branch, with no
GitHub token or API call). A folder or a repository imports every persona in it: every .md file
whose front matter gives a `name`, the others skipped. When the folder or repository holds a
divisions.json with a `divisions` object, only the folders it names are read.

Each persona keeps its front matter, plus `source:` with where it came from, so importing that
source again with --replace refreshes it. A persona whose ID or name you already have is refused,
and with --replace the one with its ID is overwritten.

A persona is prompt text that agents read and follow without asking for permission. From an
address or a repository it asks `[y/N]` first (without a terminal it refuses unless --yes);
--check lists what would be imported and saves nothing.

With --json it prints {\"personas\": [{\"id\", \"name\", \"description\", \"emoji\", \"color\", \"source\",
\"bytes\", \"path\", \"replaced\"}], \"check\", \"saved\"}.";
