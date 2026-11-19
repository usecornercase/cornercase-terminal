use std::path::Path;

use super::failed;
use super::table::OWN_ONLY;
use crate::app::App;
use crate::bots::turn;
use crate::config;
use crate::control::Item;
use crate::control::contract::Refusal;
use crate::os::fs::canonicalize;
use crate::project::Hierarchy;
use crate::project::vscode;

const NO_BOTS_FOLDER: &str = "a bot cannot open a bot's folder as a project";
const NO_BOTS_WORKTREE: &str = "a bot cannot measure a folder inside a bot's folder";
const NO_CODE_WORKSPACE: &str = "a bot cannot open a .code-workspace file: open its folders one by one";

impl App {
    pub(super) fn closes_own(&self, client: u64, item: Item) -> Result<(), Refusal> {
        let own = self.clients.bots.bot_of(client);
        let started = |term: &crate::term::Term| term.bot.is_some() && term.bot.as_deref() == own;
        let theirs = match item {
            Item::Pane(id) => self.projects.pane_by(id).is_none_or(started),
            Item::Tab(id) => self.projects.tab_index(id).is_none_or(|at| {
                let panes = &self.projects.tab(at).panes;
                !panes.is_empty() && panes.iter().all(started)
            }),
            Item::Workspace(_) | Item::Project(_) | Item::Group(_) => {
                return Err(failed(&format!("{OWN_ONLY}; ask the user to close a workspace or a project")));
            }
        };
        if theirs { Ok(()) } else { Err(failed(&format!("{OWN_ONLY}; {item} was not started by this bot"))) }
    }

    pub(super) fn bot_opens(&self, path: &Path) -> Result<(), Refusal> {
        let wanted = config::expand_home(&path.to_string_lossy(), self.home.as_deref());
        if vscode::is_workspace(&wanted) {
            return Err(failed(NO_CODE_WORKSPACE));
        }
        if self.in_bots_folder(&wanted) { Err(failed(NO_BOTS_FOLDER)) } else { Ok(()) }
    }

    pub(super) fn bot_measures(&self, path: &Path) -> Result<(), Refusal> {
        if self.in_bots_folder(path) { Err(failed(NO_BOTS_WORKTREE)) } else { Ok(()) }
    }

    fn in_bots_folder(&self, path: &Path) -> bool {
        let bots = self.panels.bots.dir.as_deref();
        canonicalize(path).is_ok_and(|path| turn::inside(bots, &path))
    }
}
