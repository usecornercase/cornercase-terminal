# glab fixtures

`1.123.0/auth-status.json` and `1.123.0/merged-requests.json` have the shape glab 1.123.0 printed for `glab auth status --all --output json` and for one page of `glab api "merge_requests?scope=created_by_me&state=merged&…"`, with hosts, logins and projects replaced (Stats' merged pull requests).

`1.124.0/` is what glab 1.124.0 printed on 2026-10-21 for public merge requests on gitlab.com, with the project (`acme/web-shop`), the people (`ana`, `luis`), titles, descriptions, ids, commits, runners and times replaced, reformatted with two spaces, and each pipeline's jobs cut to six or seven:

- `mr-view.json`: `glab mr view --output json` for an open merge request whose pipeline passed. glab prints its whole Go struct, so every key is there, `null` and empty ones included. `head_pipeline` is the merge request's latest pipeline (here a merged results pipeline, `refs/merge-requests/482/merge`); its `project_id` is the project it ran in, which for a merge request from a fork is usually the fork.
- `mr-view-conflicting.json`: a draft that conflicts with its base. `has_conflicts` is `true` while `detailed_merge_status` says `draft_status`: GitLab names only the first thing that blocks the merge, so the conflict is read from `has_conflicts`.
- `pipeline-jobs.json` and `pipeline-jobs-failed.json`: `glab api --paginate "projects/<id>/pipelines/<id>/jobs?per_page=100"` for those two pipelines, one JSON array. A manual job is `manual`, here with `allow_failure: true`; a job allowed to fail that passed is `success` with `allow_failure: true`.
- `mr-view-none.stdout` and `mr-view-none.stderr`: a branch without a merge request (exit status 1). Under `--output json` glab also writes the error to stdout, as `{"error":{"message":…}}`. Its stderr is fang's styled block even through a pipe: an `ERROR` header between blank lines, then the message with its first letter capitalized, a period, and trailing spaces.
- `mr-view-several.stdout` and `mr-view-several.stderr`: a branch name with several merge requests (any state, newest first), which glab will not pick without a prompt (exit status 1).
- `auth-status-logged-out.json`: `glab auth status --all --output json` with no login (exit status 1): gitlab.com is listed with `state: error` and `token_source: none`.
