# gh fixtures

`2.101.0/pr-view.json` has the shape `gh pr view --json number,title,url,state,isDraft,mergeable,baseRefName,statusCheckRollup` printed with **gh 2.101.0** for a real pull request, with the repository, names, ids and times replaced. `statusCheckRollup` mixes `CheckRun` items (a GitHub Actions job: `status`, `conclusion`, `detailsUrl`, `workflowName`) and `StatusContext` items (a commit status set by an app: `state`, `targetUrl`, which can be empty). The same check can be listed more than once, for example after it ran again; `gh pr checks` keeps the one started last, and so does cornercase, so the fixture lists `tests (ubuntu-24.04)` twice. A job still running has an empty `conclusion` and a zero `completedAt`.

`mergeable` is `MERGEABLE`, `CONFLICTING` or `UNKNOWN`: GitHub works it out lazily, so it answers `UNKNOWN` while it does, and always for merged and closed pull requests. `baseRefName` is the branch the pull request merges into.

`2.101.0/pr-view-conflicting.json` is a real pull request that conflicted with its base: GitHub ran none of its `pull_request` workflows, so its rollup is one app's commit status.

`2.101.0/stack.json` is the answer of `gh api graphql` for a pull request's `stack` (the query in `src/pull_request/stack.rs`), in the shape gh 2.101.0 printed for a real stack on github.com, with synthetic acme/web-shop data: entries come bottom first, `statusCheckRollup` is `null` without checks, `reviewDecision` is `null` without a review rule. GitHub answers `mergeable: CONFLICTING` for layers that were merged (seen on github/gh-stack, 2026-10-22), so only open and draft layers count as conflicting. `stack-none.json` is a pull request in no stack (`"stack": null`).

`2.101.0/stack-unsupported.json` and `stack-unsupported.stderr` are what a host without stacks answers: gh exits 1, prints the GraphQL error body on **stdout** (`extensions.code` is `undefinedField`) and `gh: Field 'stack' doesn't exist…` on stderr. It was captured on github.com with a misspelt field, since GitHub Enterprise Server was not at hand.
