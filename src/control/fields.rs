use std::collections::BTreeMap;

use super::{
    Close, Command, Mark, NewTab, NewWorkspace, Open, ProfileDelete, ProfileNew, ProfileUse, SendText, Split, Start,
    Wait,
};

impl Command {
    pub(crate) fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = Fields::default();
        let _ = self.pane_fields(&mut fields)
            || self.row_fields(&mut fields)
            || self.stream_fields(&mut fields)
            || self.terminal_fields(&mut fields)
            || self.change_fields(&mut fields)
            || self.files_fields(&mut fields)
            || self.issue_fields(&mut fields)
            || self.flow_fields(&mut fields)
            || self.account_fields(&mut fields)
            || self.server_fields(&mut fields);
        fields.0
    }

    fn pane_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::Open(open) => open.fields(fields),
            Self::NewWorkspace(new) => new.fields(fields),
            Self::NewTab(new) => new.fields(fields),
            Self::Split(split) => split.fields(fields),
            Self::Start(start) | Self::StartWith(start) => start.fields(fields),
            Self::Send(send) => send.fields(fields),
            Self::Keys(keys) => {
                fields.maybe("pane", keys.pane).maybe("tab", keys.tab).add("keys", keys.keys.len());
            }
            Self::Read(read) => {
                fields.maybe("pane", read.pane).maybe("tab", read.tab).maybe("lines", read.lines);
            }
            Self::LastMessage(last) => {
                fields.maybe("pane", last.pane).maybe("tab", last.tab);
            }
            Self::Wait(wait) => wait.fields(fields, false),
            Self::WaitSeveral(wait) => wait.fields(fields, true),
            Self::Mark(mark) => mark.fields(fields),
            Self::Notify(notify) => {
                fields.add("bytes", notify.text.len());
            }
            Self::Focus(focus) => {
                fields.add("item", focus.item);
                fields.maybe("reveal_bytes", focus.reveal.as_ref().map(|reveal| reveal.query.len()));
                fields.maybe("line", focus.reveal.as_ref().and_then(|reveal| reveal.line));
            }
            _ => return false,
        }
        true
    }

    fn row_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::Close(close) => close.fields(fields),
            Self::CloseAgent(close) => close.fields(fields),
            Self::AgentsClose(close) => close.fields(fields),
            Self::AgentsReopen(reopen) => reopen.fields(fields),
            Self::AgentsForget(forget) => forget.fields(fields),
            Self::Rename(rename) => {
                fields.maybe("item", rename.item).add("name", &rename.name);
            }
            Self::RemovalCheck(check) => {
                fields.maybe("workspace", check.workspace);
            }
            Self::Move(moving) => moving.fields(fields),
            Self::MovePane(moving) => moving.fields(fields),
            Self::Collapse(fold) | Self::Expand(fold) => {
                fields.add("item", fold.item);
            }
            Self::GroupNew(new) => {
                fields.add("name", &new.name);
            }
            Self::GroupStyle(style) => style.fields(fields),
            Self::GroupDelete(delete) => delete.fields(fields),
            Self::Todo(todo) => todo.fields(fields),
            Self::TodoSend(send) => send.fields(fields),
            _ => return false,
        }
        true
    }

    fn stream_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::Status(_)
            | Self::Stats(_)
            | Self::Contract(_)
            | Self::Watch(_)
            | Self::App(_)
            | Self::FlowRuns(_) => {}
            Self::Events(events) => events.fields(fields),
            Self::Surface(surface) => surface.fields(fields),
            Self::View(view) => view.fields(fields),
            Self::ViewSet(set) => set.fields(fields),
            Self::Search(search) => {
                fields.add("bytes", search.query.len());
            }
            _ => return false,
        }
        true
    }

    fn terminal_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::Input(input) => input.fields(fields),
            Self::Scroll(scroll) => scroll.fields(fields),
            Self::Selection(selection) => selection.fields(fields),
            Self::Resize(resize) => resize.fields(fields),
            Self::Redraw(redraw) => redraw.fields(fields),
            Self::Ratio(ratio) => ratio.fields(fields),
            Self::Link(link) => link.fields(fields),
            Self::PasteFile(paste) => paste.fields(fields),
            _ => return false,
        }
        true
    }

    fn change_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::Changes(changes) => changes.fields(fields),
            Self::ChangesMode(mode) => mode.fields(fields),
            Self::ChangesFile(file) => file.fields(fields),
            Self::ChangesGap(gap) => gap.fields(fields),
            Self::ChangesBase(base) => base.fields(fields),
            Self::ChangesViewed(viewed) => viewed.fields(fields),
            Self::ChangesDiscard(discard) => discard.fields(fields),
            Self::ChangesStage(stage) => stage.fields(fields),
            Self::ChangesMergeCheck(check) => check.fields(fields),
            _ => return false,
        }
        true
    }

    fn files_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::FilesList(list) => list.fields(fields),
            Self::FilesRead(read) => read.fields(fields),
            Self::FilesSearch(search) => search.fields(fields),
            _ => return false,
        }
        true
    }

    fn issue_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::IssueSources(sources) => sources.fields(fields),
            Self::IssueList(list) => list.fields(fields),
            Self::IssueRead(read) => read.fields(fields),
            Self::IssuePeople(people) => people.fields(fields),
            Self::Queue(_) => {
                fields.add("queue", true);
            }
            _ => return false,
        }
        true
    }

    fn flow_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::FlowList(list) => list.fields(fields),
            Self::FlowRun(run) => run.fields(fields),
            Self::FlowExport(export) => export.fields(fields),
            Self::FlowImport(import) => import.fields(fields),
            Self::FlowSave(save) => save.fields(fields),
            Self::FlowDelete(delete) => delete.fields(fields),
            Self::FlowHide(hide) => hide.fields(fields),
            Self::FlowRestore(restore) => restore.fields(fields),
            Self::FlowDefault(default) => default.fields(fields),
            Self::FlowOrder(order) => order.fields(fields),
            Self::FlowMessages(messages) => messages.fields(fields),
            Self::FlowFiles(files) => files.fields(fields),
            _ => return false,
        }
        true
    }

    fn account_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::AccountList(list) => list.fields(fields),
            Self::AccountAdd(add) => add.fields(fields),
            Self::AccountRemove(remove) => remove.fields(fields),
            Self::AccountRename(rename) => rename.fields(fields),
            Self::AccountUse(chosen) => chosen.fields(fields),
            Self::AccountRestart(restart) => restart.fields(fields),
            Self::ProfileList(_) => {}
            Self::ProfileSet(set) => set.fields(fields),
            Self::ProfileNew(ProfileNew { name, .. })
            | Self::ProfileUse(ProfileUse { name })
            | Self::ProfileDelete(ProfileDelete { name }) => {
                fields.add("name", name);
            }
            _ => return false,
        }
        true
    }

    fn server_fields(&self, fields: &mut Fields) -> bool {
        match self {
            Self::Go(_)
            | Self::UpdateCheck(_)
            | Self::UpdateInstall(_)
            | Self::PluginList(_)
            | Self::ConfigExport(_) => {}
            Self::Config(config) => config.fields(fields),
            Self::ConfigImport(import) => import.fields(fields),
            Self::Theme(theme) => theme.fields(fields),
            Self::RestartWhenIdle(restart) => {
                fields.maybe("timeout", restart.timeout);
            }
            Self::StartServer(start) => start.fields(fields),
            Self::Away(away) => {
                fields.add("off", away.off);
            }
            Self::Usage(usage) => usage.fields(fields),
            Self::Ports(ports) => ports.fields(fields),
            Self::StopPort(stop) => stop.fields(fields),
            Self::WorktreeSize(size) => size.fields(fields),
            Self::FreeBuilds(free) => free.fields(fields),
            Self::Tour(tour) => tour.fields(fields),
            Self::PluginActions(actions) => actions.fields(fields),
            Self::PluginRun(run) => run.fields(fields),
            _ => return false,
        }
        true
    }
}

impl Open {
    fn fields(&self, fields: &mut Fields) {
        fields.add("path", self.path.display()).add("focus", self.focus);
    }
}

impl NewWorkspace {
    fn fields(&self, fields: &mut Fields) {
        fields.add("name", &self.name).maybe("project", self.project).add("worktree", self.worktree);
        fields.add("focus", self.focus);
    }
}

impl NewTab {
    fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).maybe("name", self.name.as_ref());
        fields.add("command", self.command.is_some()).add("focus", self.focus);
        fields.maybe("profile", self.profile.as_ref()).maybe("accounts", chosen(&self.accounts));
    }
}

impl Split {
    fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).add("down", self.down).add("command", self.command.is_some());
        fields.add("focus", self.focus);
    }
}

impl Start {
    fn fields(&self, fields: &mut Fields) {
        fields.maybe("agent", self.agent.as_ref()).maybe("model", self.model.as_ref());
        fields.maybe("mode", self.mode.as_ref()).maybe("effort", self.effort.as_ref());
        fields.maybe("worktree", self.worktree.as_ref());
        fields.maybe("project", self.project);
        fields.maybe("workspace", self.workspace).add("prompt", self.prompt.is_some());
        fields.add("issue", self.issue.is_some()).maybe("source", self.source.as_ref());
        fields.add("wait", self.wait).maybe("until", self.wait.then(|| self.until.name()));
        fields.maybe("timeout", self.timeout).add("focus", self.focus);
        fields.maybe("profile", self.profile.as_ref()).maybe("accounts", chosen(&self.accounts));
    }
}

impl SendText {
    fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).maybe("tab", self.tab);
        fields.add("bytes", self.text.as_ref().map_or(0, String::len)).add("enter", self.enter);
        fields.add("wait", self.wait).maybe("until", self.wait.then(|| self.until.name()));
        fields.maybe("timeout", self.timeout).add("force", self.force);
    }
}

impl Wait {
    fn fields(&self, fields: &mut Fields, several: bool) {
        fields.maybe("pane", self.pane).maybe("tab", self.tab).add("until", self.until.name());
        fields.maybe("timeout", self.timeout);
        if several {
            fields.add("panes", listed(&self.panes)).add("tabs", listed(&self.tabs));
            fields.add("mode", if self.all { "all" } else { "any" });
        }
    }
}

impl Mark {
    fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).maybe("tab", self.tab).add("reviewed", self.reviewed);
    }
}

impl Close {
    fn fields(&self, fields: &mut Fields) {
        fields.add("item", self.item).add("remove_worktree", self.remove_worktree).add("force", self.force);
        fields.add("unlock", self.unlock);
    }
}

pub(super) fn listed(ids: &[u64]) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

#[derive(Default)]
pub(super) struct Fields(pub(super) Vec<(&'static str, String)>);

impl Fields {
    pub(super) fn add(&mut self, key: &'static str, value: impl std::fmt::Display) -> &mut Self {
        self.0.push((key, value.to_string()));
        self
    }

    pub(super) fn maybe(&mut self, key: &'static str, value: Option<impl std::fmt::Display>) -> &mut Self {
        if let Some(value) = value {
            self.add(key, value);
        }
        self
    }
}

pub(super) fn chosen(accounts: &BTreeMap<String, String>) -> Option<String> {
    (!accounts.is_empty())
        .then(|| accounts.iter().map(|(agent, account)| format!("{agent}={account}")).collect::<Vec<_>>().join(","))
}
