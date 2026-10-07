## What

<!-- What does this change do? One or two sentences. -->

## Why

<!-- The problem it solves or the issue it closes (Closes #123). -->

## How

<!-- The approach, and anything a reviewer should look at closely. Mention alternatives you discarded and why. -->

## Testing

<!-- New or updated tests, and what you checked by hand (terminal, size, compact mode…). -->

## Screenshots

<!-- For anything visible: before / after, or a short recording. Delete this section otherwise. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy --all-targets --all-features --locked -- -D warnings` and `cargo test --locked` pass
- [ ] `cargo-machete` and `npx -y jscpd@4.3.0` pass
- [ ] Changed snapshots were reviewed, not just accepted
- [ ] No new keyboard shortcuts, or they were discussed in an issue first
- [ ] `DESIGN.md` is updated if this changes a design decision, `AGENTS.md` if it adds a module or changes a rule
- [ ] The website (`site/`) is updated if this adds, changes or removes a feature
