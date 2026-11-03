# Folder picker

Code: `project/picker.rs`, `list.rs`, `app/picker_dialog.rs`

- Starts in the parent of the active project (of the group's first project for **add project**). `Enter` goes into the selected folder (or imports the selected `.code-workspace`), or opens the current one when nothing is selected. Typing a path walks it; a paste goes through the same path (e2e opens projects this way). Folders are read on the main thread.
- **Typing a path on Windows** (`Picker::push`): a separator is whatever `std::path::is_separator` takes, so `\` and `/` both go into the selected folder there, and `/` alone on Unix, as before. With the filter empty a separator goes to the root of the current folder's drive (`C:\`), where Unix goes to `/`; a filter that is a drive (`d:`) and then a separator goes to that drive's root (`D:\`). So a typed or pasted `C:\Users\ana\code\web` walks as `/home/ana/code/web` does. Nothing goes to the network: `\\server` walks from the current drive's root and finds no folder named `server`, so the picker never names a share.
