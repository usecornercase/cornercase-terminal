# Contributing

Thanks for helping. Bug reports, ideas and pull requests are all welcome. Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

For anything bigger than a small fix, open an issue first so we can agree on the approach. cornercase has a few deliberate choices that are easy to break by accident:

- **Mouse buttons only.** Every key goes to the program in the active pane; the app has no keyboard shortcuts of its own. Please do not add any without discussing it first.
- **The server owns the shells.** The UI is a thin client that only writes frames, so behaviour belongs in the server (`App`), not in `client.rs`.

The rules for every change are in [AGENTS.md](AGENTS.md), and the design and the reasons behind it in [DESIGN.md](DESIGN.md). Read the section for the area you touch.

## Setup

You need Linux or macOS (with the Xcode Command Line Tools), [rustup](https://rustup.rs) (it installs the pinned Rust version by itself), `git` and [Zig 0.15.2](https://ziglang.org/download/) on your `PATH` (exactly this version). The first build fetches Ghostty's sources and takes a couple of minutes.

```sh
cargo run                  # build and attach
cargo run -- kill-server   # needed after every rebuild: a client never attaches to a server from another build
```

To experiment without touching your real session, use a separate socket:

```sh
CORNERCASE_SOCKET=/tmp/cc-dev/server.sock cargo run
```

## Checks

CI runs all of these; please run them before opening a pull request:

```sh
cargo fmt
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo-machete              # cargo install cargo-machete
npx -y jscpd@4.3.0         # copy-paste detector
```

UI changes usually change a snapshot. `cargo test` then writes `src/snapshots/*.snap.new` and fails; check the new render and accept it with `INSTA_UPDATE=always cargo test`.

## Code style

- Everything in English: identifiers, test names, messages, UI text.
- No code comments, including doc comments. If a decision needs explaining, add it to `DESIGN.md`.
- No `unwrap()` outside tests. Errors use `crate::error::Error`.
- Tests are named as sentences (`close_button_closes_its_entry`) and test one behaviour each. A test added as a safety net must fail without the code it protects.

## Pull requests

Keep them focused: one change per pull request. Describe what changed and why, how you tested it, and add a screenshot or a recording for anything visible.

A pull request that changes the app (`src/`, `Cargo.toml`, `Cargo.lock`, the toolchain) ships as a release of its own once merged. So it bumps `version` in `Cargo.toml` above the last release (usually the patch number) and adds a `## <version>` section to [CHANGELOG.md](CHANGELOG.md) saying what changed, written for users: it becomes the release notes and the text of the update dialog in the app. CI checks both. Changes to docs, tests or the website alone need neither.

By contributing, you agree that your contributions are dual licensed under MIT and Apache-2.0, like the rest of the project.
