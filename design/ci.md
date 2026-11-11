# CI

Code: `.github/workflows/ci.yml`, #753

- **Time limits** (#753): each `clippy + tests` job has a limit sized to what a passing run takes on its runner, written beside the runner in the matrix. A job that reaches its limit ends as cancelled, and a cancelled required check blocks the merge until someone reruns it, and the rerun can be cancelled again; flows that merge on their own and Dependabot's pull requests wait on it like everyone else.
  - **macOS has an hour.** By 2026-11-11 a passing run took 25 to 30 minutes (the unit tests alone about 17 on the runner's 3 cores, where the whole Linux job takes about 12), and a Dependabot run, which rebuilds what it bumps, a few minutes more; from 2026-11-09 to 2026-11-11 a quarter of its runs reached the old limit, 30, and were cancelled. The job had grown by about 5 minutes a week since mid-October, as features bring their tests, so 45, the Windows job's, would have been reached again within a few weeks, and sooner by a run from a cold cache, which builds Ghostty and every dependency again.
  - **Linux keeps 30**, about twice what a passing run took by 2026-11-11, so a run that hangs there still ends at 30.
  - **Windows has 45** (#601), so that a run from a cold cache (Ghostty with Zig, then every dependency checked for clippy and compiled for the tests) fits; a warm one took about 8 minutes by 2026-11-11.
  - A longer limit does not make the tests faster: that is #754 (finding the slow tests, the test threads, splitting the job), after which the macOS limit can come back down.
  - Rejected: one limit for the whole matrix, which would let a Linux run that hangs hold its runner twice as long; and picking the limit with an expression on the runner's name (`startsWith(matrix.os, 'macos') && 60 || 30`), which keeps the limit away from its runner in an `&& ||` idiom that actionlint cannot type, where an `include` entry states it as a number beside the runner.
  - Checked (2026-11-11): `actionlint` 1.7.7 accepts the matrix and reads `timeout` as a number; the jobs keep their names, which `main`'s ruleset requires.
