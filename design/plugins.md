# Plugins

Code: `plugins/`, `app/plugins.rs`, `cli/plugin.rs`, `client/window.rs`

- **What a plugin is** (PR 1 of 3 is the core; PR 2 adds installing, below; the catalogue is PR 3): a folder with `cornercase-plugin.toml` that declares argv commands, modelled on herdr's plugins (`herdr-plugin.toml`). The plugin's API is the existing commands for scripts and agents: it calls `$CORNERCASE_BIN status --json`, `events --json`, `send`, `notify`… No SDK and no runtime inside cornercase (WASM, Lua, JS were rejected): every improvement to the commands is one to plugins at no cost, and a plugin can be written in any language. TOML, like herdr's, so plugins port easily; it costs the `toml` crate (parse and serde only). Entries: `[[actions]]` (one-shot, in the menus), `[[events]]` (one-shot per event), `[[service]]` (kept running by the server), `[[window]]` (kept running by each local window), and for installing `[[build]]` and `[download.<target>]` (see Installing). Left out: `[[tab]]` (`new-tab --command` covers it), link handlers, per-entry `platforms`, plugin-defined settings, and surfaces inside the TUI (sidebar areas, panel tabs).
- **Tests**: fake plugins are `/bin/sh` scripts in temporary folders (`plugins::*::tests`, `app::tests::plugins`, `e2e::plugins`); supervision is tested on the real thread with a service that records its pid, killed, disabled and dropped.

## Topics

- [The manifest, commands and the registry](plugins/manifest.md): The manifest; Commands are argv, never a shell; The registry.
- [Turning plugins on, permissions, environment and folders](plugins/permissions-and-environment.md): Nothing runs until the user turns it on; Permissions are shown, not enforced; What a command inherits; Folders and the log.
- [Actions, hooks, services and running files](plugins/running.md): Actions in the menus; Event hooks; Services and window processes; Running files; The server loop never waits on a plugin; Settings › Plugins; The commands.
- [Installing](plugins/installing.md): sources, fetching, the preview, installing, updating, removing, `[download]`, git, the review.
- [Finding plugins](plugins/finding-plugins.md): the crawl, the blocklist, the index, trending and new, the page and its CSP.
- [Through the server](plugins/through-the-server.md): #343: `plugin-list`, `plugin-actions`, `plugin-run`.
