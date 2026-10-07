# cornercase

A terminal multiplexer for working on several projects at once, each with its own git worktrees and coding agents. Every action is a mouse button, so you never need to learn a prefix key.

> **Status:** early and moving fast. Runs on Linux and macOS.

**Website and documentation:** https://usecornercase.dev

## What it does

- **Projects, workspaces, tabs.** A sidebar of projects (folders), and for the active project its workspaces (lines of work) and their tabs. Click to switch, `+` to create, `×` to close, right-click to rename, drag to reorder.
- **Git worktrees.** A new workspace in a git repository can get its own worktree and branch. Worktrees created outside cornercase show up too. Files listed in `.worktreeinclude` (such as `.env`) are copied into new checkouts.
- **Splits.** Right-click a pane to split it right or down; drag the dividers to resize.
- **Issues to agents.** Browse GitHub issues, Shortcut stories, Linear issues and Jira issues, read them as Markdown, and start one: cornercase creates a worktree on a matching branch, launches your coding agent (Claude Code, Codex, Gemini, …) in a new tab and hands it the issue.
- **Agent status.** A tab running Claude Code shows whether it is working (`◐`), needs you (`!`) or finished while you were elsewhere (`✓`), and under its name the model and how full its context is (`Opus 5.5 · 23%`). `!` and `✓` also mark its workspace and project, so an agent waiting in another project doesn't go unnoticed.
- **Codex context.** Codex tabs also show their model and context use (`gpt-5.4 · 20%`), using each pane's own session and Codex's reported window size. When usage is unavailable, they show the model alone. Works with native and npm installations and with Codex's background app-server; respects `CODEX_HOME`. Verified against Codex CLI 0.160.0.
- **Scripts and agents.** The `cornercase` command drives the running server: it lists everything with ids, opens tabs and worktrees, types into panes, starts agents and hands them a prompt, waits until they finish or need you, and reads what they wrote. So an agent in one tab can coordinate others. A [skill](skills/cornercase/SKILL.md) teaches your agents to use it.
- **Sessions survive the UI.** A background server owns the shells. Closing the window or clicking ` quit ` detaches; running `cornercase` again reattaches. Several terminals can attach at once and mirror each other.
- **A real terminal inside.** Panes are emulated with [libghostty-vt](https://github.com/ghostty-org/ghostty), Ghostty's terminal core, so nvim, fzf, htop and full-screen agents work as expected.
- **Responsive.** Below 90 columns the sidebars fold into a menu bar.

## Install

```sh
curl -fsSL https://usecornercase.dev/install.sh | sh
```

or with Homebrew:

```sh
brew install usecornercase/tap/cornercase
```

Prebuilt binaries cover Linux and macOS on x86_64 and arm64.

### Updates

Every hour cornercase asks GitHub for the latest release. When there is a newer one, a ` ↑ 0.2.0 ` button shows up next to ` settings `: it shows what is new since your version, downloads the new binary, checks its checksum and replaces the old one, then offers to restart. Your session comes back after the restart, with new shells in the same folders. Homebrew installs show `brew upgrade cornercase` instead. Turn the check off in ` settings ` → TUI.

### From source

You need:

- Linux or macOS (on macOS, the Xcode Command Line Tools: `xcode-select --install`)
- [rustup](https://rustup.rs) (it installs the Rust version pinned in `rust-toolchain.toml` on the first build)
- [Zig 0.15.2](https://ziglang.org/download/) on your `PATH`, exactly this version: it builds Ghostty's terminal core, which refuses any other. Package managers such as Homebrew may ship a newer one, so download it from ziglang.org if in doubt.
- `git`, and network access for the first build (it fetches Ghostty's sources)

```sh
git clone https://github.com/usecornercase/cornercase-terminal.git
cd cornercase-terminal
cargo install --path . --locked
```

The first build takes a couple of minutes.

## Usage

```sh
cornercase              # open the UI (starts the server if needed)
cornercase update       # install the latest release and restart the server
cornercase kill-server  # stop the server and every shell in it
cornercase --version    # print the version
cornercase --help       # every command, including the ones for scripts and agents
```

For example, from a script or an agent in one of cornercase's panes:

```sh
pane=$(cornercase start claude --worktree fix-login --prompt 'Fix the login form, then commit')
cornercase wait --pane "$pane" --timeout 1800   # until the agent stops working: idle, done or waiting
cornercase read --pane "$pane" --lines 40
```

See [Scripts and agents](https://usecornercase.dev/docs/guides/scripts-and-agents/) for every command. Install the skill for your agents with `npx skills add usecornercase/cornercase-terminal --skill cornercase -g`.

On a fresh server, cornercase reopens your last session: the same projects, workspaces, tabs and splits, each pane with a new shell in its folder.

Keys always go to the program in the active pane. While a form or modal is open, `Enter` confirms and `Esc` cancels.

To copy text, drag over it in a pane. cornercase sends the selection to your terminal's clipboard with OSC 52, which some terminals need you to allow (iTerm2: *Applications in terminal may access clipboard*; tmux: `set -g set-clipboard on`).

## Configuration

Open ` settings ` in the sidebar. Changes are saved at once to `~/.config/cornercase/config.json` (`$XDG_CONFIG_HOME` is respected).

| Where | What |
| --- | --- |
| `~/.config/cornercase/config.json` | settings: worktrees folder, issue sources, agent and its arguments, prompt |
| `~/.config/cornercase/secrets.json` | Shortcut, Linear and Jira tokens typed in the app (mode 0600) |
| `~/.local/state/cornercase/session.json` | the saved session |
| `$XDG_RUNTIME_DIR/cornercase/`, or `$TMPDIR/cornercase-<uid>/` | the server's socket and `server.log` |

Environment variables:

- `SHORTCUT_API_TOKEN`, `LINEAR_API_KEY`, `JIRA_API_TOKEN`: tokens for the issues modal, taking precedence over saved ones.
- `CORNERCASE_SOCKET`: run a separate server on another socket (its config and session live next to it).
- `CORNERCASE_RELEASES_URL`: where the update check looks for the latest release (GitHub's API by default).

GitHub issues are read with the [`gh`](https://cli.github.com) CLI, using its login.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Design notes and the reasons behind them are in [CLAUDE.md](CLAUDE.md). The website and its documentation live in [`site/`](site).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
