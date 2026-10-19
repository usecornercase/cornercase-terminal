# Folder picker

Code: `project/picker.rs`, `list.rs`, `app/picker_dialog.rs`

- Starts in the parent of the active project (of the group's first project for **add project**). `Enter` goes into the selected folder (or imports the selected `.code-workspace`), or opens the current one when nothing is selected. Typing a path walks it; a paste goes through the same path (e2e opens projects this way). Folders are read on the main thread.
