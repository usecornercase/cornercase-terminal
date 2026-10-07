# Codex context screen

`codex-context.ansi` captures the real `App::draw` output while `test_util::FakeCodex` runs in a PTY in a temporary `shop` repository. The fixture is Codex CLI 0.160.0's reduced rollout in `tests/fixtures/codex/0.160.0/context.jsonl`, with the model `gpt-5.4` and 20% context use. No real agent or account is involved.

Captured at 110 columns × 24 rows using a ratatui test backend and exported as ANSI cells. The environment denied tmux's local socket, so this capture uses the actual application renderer directly. The documentation crops it to the project and workspace columns. Regenerate with the fake-agent setup in `app::tests::agent_status` and `App::draw`; do not edit the displayed text by hand. For 0.11.1 its styling was updated in place to the new look (tree lines, the left rail, the quieter line colour), like the other screens.
