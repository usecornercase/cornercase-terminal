# Layout

Code: `config.rs`, `state.rs`, `ui.rs`, `ui/`, `app/layout.rs`, `app/sidebar.rs`, `app/tab_bar.rs`, `app/interaction.rs`

- `sidebar` in `config.json` lays out the lists (wide mode only; below 90 columns the compact menu below applies to every value): `projects_on_top` (the default since 0.9.0, `config::Sidebar`'s `#[default]`, so unknown values fall back to it), `workspaces_on_top`, `side_by_side` and `tree`. One column was made the default because it gives the panes the most room; a `config.json` that names `side_by_side` keeps it (settings save the whole file, so anyone who changed any setting before 0.9.0 keeps three columns). App tests pin `side_by_side` in `new_app`, since their geometry helpers assume it; `tests/e2e/main.rs` lays out its geometry with `Sidebar::default()`, so e2e covers the shipped default.
- Side by side: projects (32), workspaces (26), pane (after one blank column). Columns are always present so PTY sizes do not jump. Borders drag to resize (double click resets), within `MIN_COLUMN_WIDTH` and `MIN_PANE_WIDTH`; `Widths::fit` squeezes them on small terminals. Widths are saved in the session. PTYs follow the drag live.
- **Compact mode** below 90 columns (`ui::layout::compact_layout`): a top bar (` ≡ `, `project › workspace › tab`, then ` ± `, ` ▤ `, ` ☐ `, ` ⌕ ` right-aligned; ` ± ` only in git workspaces and ` ▤ ` only with a project, so their slots are left of ` ☐ ` and an empty one merges into the breadcrumb; ` ☐ ` gets a muted `●` to its right, in its own slot, while TODO items are pending, see TODO panel) and a full-screen menu showing one column at a time. Every clickable item is a 3-row band (`Areas::pitch`). Both columns' areas share one rect and `Areas::shown(nav)` blanks the hidden one, so drawing and hit testing reuse the wide-mode code. With the agents section on, it is a third menu (`Nav::Agents`): ` Agents › ` at the right of the projects menu's title opens it, ` ‹ Projects ` (the same `Areas::back`) goes back, and a click on a row closes the menu on that pane.

## Topics

- [Stacked sidebar and tree](layout/sidebar.md): the stacked layouts and their line; the tree's rows, arrows and drops.
- [The agents section](layout/agents-section.md): Agents section; The agents section is a tree: its order, drawing, roles and folding.
- [Resize pointer](layout/resize-pointer.md): OSC 22 shapes over borders and dividers, which terminals get them, multiplexers, leaving.
- [Row menus, the account row and counts](layout/row-menus-and-account-row.md): Row menus; The account row under each pane; Counts.
- [Tabs above the pane and the icon bar](layout/tab-bar-and-icon-bar.md): Tabs above the pane; Icon bar: icons, places, indicators, names on hover, colours.
- [Side panels, folding, scrolling and the active row](layout/panels-and-folding.md): Side panels; Folding; scrolling lists by item; the active row and its rail.
- [Lines, wording and the colours of text](layout/lines-and-text.md): Lines; Tree lines; Empty panes; Capitals; Expand and collapse; Grey text; Soft text; the hover fill.
