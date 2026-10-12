# Saved session fixtures

`state::save` writes `session.json`. `v5.json` pins every version 5 field, including omitted defaults. When the session version changes, keep it as a migration test and add a new fixture for the exact text check.

A change to the saved format updates the fixture in the same PR. The paths and accounts are synthetic.
