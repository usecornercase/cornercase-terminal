use std::collections::BTreeMap;

use super::*;

impl Command {
    #[expect(clippy::too_many_lines, reason = "one arm per request")]
    pub(crate) fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = Fields::default();
        match self {
            Self::Queue(_) => drop(fields.add("queue", true)),
            Self::Status(_)
            | Self::Stats(_)
            | Self::Contract(_)
            | Self::Watch(_)
            | Self::Go(_)
            | Self::UpdateCheck(_)
            | Self::UpdateInstall(_)
            | Self::PluginList(_)
            | Self::FlowRuns(_)
            | Self::App(_)
            | Self::ProfileList(_) => {}
            Self::Open(open) => {
                fields.add("path", open.path.display()).add("focus", open.focus);
            }
            Self::NewWorkspace(new) => {
                fields.add("name", &new.name).maybe("project", new.project).add("worktree", new.worktree);
                fields.add("focus", new.focus);
            }
            Self::NewTab(new) => {
                fields.maybe("workspace", new.workspace).maybe("name", new.name.as_ref());
                fields.add("command", new.command.is_some()).add("focus", new.focus);
                fields.maybe("profile", new.profile.as_ref()).maybe("accounts", chosen(&new.accounts));
            }
            Self::Split(split) => {
                fields.maybe("pane", split.pane).add("down", split.down).add("command", split.command.is_some());
                fields.add("focus", split.focus);
            }
            Self::Start(start) | Self::StartWith(start) => {
                fields.maybe("agent", start.agent.as_ref()).maybe("model", start.model.as_ref());
                fields.maybe("mode", start.mode.as_ref()).maybe("effort", start.effort.as_ref());
                fields.maybe("worktree", start.worktree.as_ref());
                fields.maybe("project", start.project);
                fields.maybe("workspace", start.workspace).add("prompt", start.prompt.is_some());
                fields.add("issue", start.issue.is_some()).maybe("source", start.source.as_ref());
                fields.add("wait", start.wait).maybe("until", start.wait.then(|| start.until.name()));
                fields.maybe("timeout", start.timeout).add("focus", start.focus);
                fields.maybe("profile", start.profile.as_ref()).maybe("accounts", chosen(&start.accounts));
            }
            Self::Send(send) => {
                fields.maybe("pane", send.pane).maybe("tab", send.tab);
                fields.add("bytes", send.text.as_ref().map_or(0, String::len)).add("enter", send.enter);
                fields.add("wait", send.wait).maybe("until", send.wait.then(|| send.until.name()));
                fields.maybe("timeout", send.timeout).add("force", send.force);
            }
            Self::Keys(keys) => {
                fields.maybe("pane", keys.pane).maybe("tab", keys.tab).add("keys", keys.keys.len());
            }
            Self::Read(read) => {
                fields.maybe("pane", read.pane).maybe("tab", read.tab).maybe("lines", read.lines);
            }
            Self::LastMessage(last) => {
                fields.maybe("pane", last.pane).maybe("tab", last.tab);
            }
            Self::Wait(wait) | Self::WaitSeveral(wait) => {
                fields.maybe("pane", wait.pane).maybe("tab", wait.tab).add("until", wait.until.name());
                fields.maybe("timeout", wait.timeout);
                if let Self::WaitSeveral(_) = self {
                    fields.add("panes", listed(&wait.panes)).add("tabs", listed(&wait.tabs));
                    fields.add("mode", if wait.all { "all" } else { "any" });
                }
            }
            Self::Close(close) => {
                fields.add("item", close.item).add("remove_worktree", close.remove_worktree).add("force", close.force);
                fields.add("unlock", close.unlock);
            }
            Self::Rename(rename) => {
                fields.maybe("item", rename.item).add("name", &rename.name);
            }
            Self::Focus(focus) => {
                fields.add("item", focus.item);
            }
            Self::Notify(notify) => {
                fields.add("bytes", notify.text.len());
            }
            Self::Todo(todo) => todo.fields(&mut fields),
            Self::TodoSend(send) => send.fields(&mut fields),
            Self::Events(events) => events.fields(&mut fields),
            Self::RestartWhenIdle(restart) => {
                fields.maybe("timeout", restart.timeout);
            }
            Self::Away(away) => {
                fields.add("off", away.off);
            }
            Self::Mark(mark) => {
                fields.maybe("pane", mark.pane).maybe("tab", mark.tab).add("reviewed", mark.reviewed);
            }
            Self::FlowList(list) => list.fields(&mut fields),
            Self::FlowRun(run) => run.fields(&mut fields),
            Self::FlowExport(export) => export.fields(&mut fields),
            Self::FlowImport(import) => import.fields(&mut fields),
            Self::Search(search) => {
                fields.add("bytes", search.query.len());
            }
            Self::Surface(surface) => surface.fields(&mut fields),
            Self::View(view) => view.fields(&mut fields),
            Self::ViewSet(set) => set.fields(&mut fields),
            Self::Input(input) => input.fields(&mut fields),
            Self::Scroll(scroll) => scroll.fields(&mut fields),
            Self::Selection(selection) => selection.fields(&mut fields),
            Self::Resize(resize) => resize.fields(&mut fields),
            Self::Redraw(redraw) => redraw.fields(&mut fields),
            Self::Ratio(ratio) => ratio.fields(&mut fields),
            Self::Link(link) => link.fields(&mut fields),
            Self::StartServer(start) => start.fields(&mut fields),
            Self::Theme(theme) => theme.fields(&mut fields),
            Self::Config(config) => config.fields(&mut fields),
            Self::Move(moving) => moving.fields(&mut fields),
            Self::MovePane(moving) => moving.fields(&mut fields),
            Self::Collapse(fold) | Self::Expand(fold) => {
                fields.add("item", fold.item);
            }
            Self::GroupNew(new) => {
                fields.add("name", &new.name);
            }
            Self::GroupStyle(style) => style.fields(&mut fields),
            Self::GroupDelete(delete) => delete.fields(&mut fields),
            Self::RemovalCheck(check) => {
                fields.maybe("workspace", check.workspace);
            }
            Self::IssueSources(sources) => sources.fields(&mut fields),
            Self::IssueList(list) => list.fields(&mut fields),
            Self::IssueRead(read) => read.fields(&mut fields),
            Self::IssuePeople(people) => people.fields(&mut fields),
            Self::Changes(changes) => changes.fields(&mut fields),
            Self::ChangesMode(mode) => mode.fields(&mut fields),
            Self::ChangesFile(file) => file.fields(&mut fields),
            Self::ChangesGap(gap) => gap.fields(&mut fields),
            Self::ChangesBase(base) => base.fields(&mut fields),
            Self::ChangesViewed(viewed) => viewed.fields(&mut fields),
            Self::Usage(usage) => usage.fields(&mut fields),
            Self::PluginActions(actions) => actions.fields(&mut fields),
            Self::PluginRun(run) => run.fields(&mut fields),
            Self::FlowSave(save) => save.fields(&mut fields),
            Self::FlowDelete(delete) => delete.fields(&mut fields),
            Self::FlowHide(hide) => hide.fields(&mut fields),
            Self::FlowRestore(restore) => restore.fields(&mut fields),
            Self::FlowMessages(messages) => messages.fields(&mut fields),
            Self::FlowFiles(files) => files.fields(&mut fields),
            Self::AccountList(list) => list.fields(&mut fields),
            Self::AccountAdd(add) => add.fields(&mut fields),
            Self::AccountRemove(remove) => remove.fields(&mut fields),
            Self::ProfileSet(set) => set.fields(&mut fields),
            Self::AccountRename(rename) => rename.fields(&mut fields),
            Self::AccountUse(chosen) => chosen.fields(&mut fields),
            Self::ProfileNew(ProfileNew { name, .. })
            | Self::ProfileUse(ProfileUse { name })
            | Self::ProfileDelete(ProfileDelete { name }) => {
                fields.add("name", name);
            }
        }
        fields.0
    }
}

fn listed(ids: &[u64]) -> String {
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
