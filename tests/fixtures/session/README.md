# Saved session fixtures

`persist::save` writes `session.json`. `v5.json` pins every version 5 field, including omitted defaults. `parents.json` pins the optional `id` and `parent` of panes, which the v5 fixture leaves out. `closed.json` pins a workspace's closed agents (`closed`, with a pane's `closed` agent and a kept flow run), which keep the session version at 5. When the session version changes, keep it as a migration test and add a new fixture for the exact text check.

A change to the saved format updates the fixture in the same PR. The paths and accounts are synthetic.
