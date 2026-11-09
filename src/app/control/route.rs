use std::ops::ControlFlow;
use std::time::Instant;

use ratatui::layout::Rect;
use serde_json::Value;

use super::{Asked, Asker, Handled, contract, json, malformed, relay};
use crate::app::App;
use crate::control::contract::{Code, Refusal};
use crate::control::{self, Command, Request};

fn stale_boot(boot: &str) -> Refusal {
    let current = control::server_token();
    Refusal::new(
        Code::StaleBoot,
        format!(
            "this request is for the cornercase server started as {boot}, but this one started as {current}; \
            its ids may name other items now, so read `cornercase status` again"
        ),
    )
}

impl App {
    pub(super) fn handle_request(
        &mut self,
        client: u64,
        request: Request,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        if let Some(boot) = request.boot.as_deref().filter(|boot| *boot != control::server_token()) {
            return Err(stale_boot(boot));
        }
        let caller = request.caller.filter(|_| request.server.as_deref() == Some(control::server_token()));
        if let Some(caller) = caller {
            self.clients.requests.callers.insert(client, caller);
        }
        let asker = Asker { caller, view: request.view };
        self.check_view(&request)?;
        self.settle_views();
        let noted = self.flow_steps(caller, &request.command, now);
        let handled = match self.route(Asked { client, asker, area, now }, request.command) {
            ControlFlow::Break(handled) => handled,
            ControlFlow::Continue(command) => Err(malformed(format!("no area answers `{}`", command.name()))),
        };
        self.flow_requested(client, noted, handled.is_ok());
        handled
    }

    fn route(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let command = self.pane_requests(asked, command)?;
        let command = self.row_requests(asked, command)?;
        let command = self.stream_requests(asked, command)?;
        let command = self.surface_requests(asked, command)?;
        let command = self.changes_requests(asked, command)?;
        let command = self.files_requests(asked, command)?;
        let command = self.issue_requests(asked, command)?;
        let command = self.flow_requests(asked, command)?;
        let command = self.account_requests(asked, command)?;
        self.server_requests(asked, command)
    }

    fn pane_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::Open(open) => self.open_request(asker, &open, area),
            Command::Clone(clone) => self.clone_request(asked, clone),
            Command::NewWorkspace(new) => self.new_workspace_request(client, asker, new, area),
            Command::NewTab(new) => self.new_tab_request(asked, new),
            Command::RunSetup(run) => self.run_setup_request(asked, &run),
            Command::Split(split) => self.split_request(asked, split),
            Command::Start(start) => self.start_request(asked, start),
            Command::StartWith(start) => self.start_with_request(asked, start),
            Command::Send(send) => self.send_request(client, asker, send, now),
            Command::Keys(keys) => self.keys_request(asker, &keys, now),
            Command::Held(held) => self.held_request(asker, &held),
            Command::Read(read) => self.read_request(asker, &read),
            Command::LastMessage(last) => self.last_message_request(client, asker, &last),
            Command::Wait(wait) => self.wait_request(client, asker, wait, now),
            Command::WaitSeveral(wait) => self.wait_several_request(client, asker, wait, now),
            Command::Mark(mark) => self.mark_request(asker, &mark),
            Command::Notify(notify) => self.notify_request(&notify.text),
            Command::Focus(focus) => self.focus_request(asker, &focus),
            Command::FocusMachine(focus) => self.focus_machine_request(&focus),
            other => return ControlFlow::Continue(other),
        })
    }

    fn row_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::Close(close) => self.close_request(client, &close),
            Command::Rename(rename) => self.rename_request(asker, rename),
            Command::RemovalCheck(check) => self.removal_check_request(client, &check),
            Command::CloseAgent(close) => self.close_agent_request(&close),
            Command::AgentsClose(close) => self.agents_close_request(asker, &close),
            Command::AgentsReopen(reopen) => self.agents_reopen_request(asker, &reopen, area),
            Command::AgentsForget(forget) => self.agents_forget_request(asker, &forget),
            Command::Move(moving) => self.move_request(&moving),
            Command::MovePane(moving) => self.move_pane_request(&moving, area),
            Command::Collapse(fold) => self.fold_request("collapse", fold.item, true),
            Command::Expand(fold) => self.fold_request("expand", fold.item, false),
            Command::GroupNew(new) => self.group_new_request(&new),
            Command::GroupStyle(style) => self.group_style_request(&style),
            Command::GroupDelete(delete) => self.group_delete_request(&delete),
            Command::Todo(todo) => self.todo_request(todo),
            Command::TodoSend(send) => self.todo_send_request(client, asker, &send, now),
            other => return ControlFlow::Continue(other),
        })
    }

    fn stream_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::Status(_) => Ok(Some(self.status_value(asker))),
            Command::Stats(stats) => self.stats_request(client, &stats, now),
            Command::Contract(_) => Ok(Some(json(&contract::descriptor(self.unavailable())))),
            Command::Watch(_) => Ok(Some(json(&self.watch_snapshots(client, now)))),
            Command::App(_) => Ok(Some(json(&self.app_snapshots(client, now)))),
            Command::Events(events) => self.events_request(client, events),
            Command::FlowRuns(_) => Ok(Some(json(&self.flow_runs_snapshots(client, now)))),
            Command::Surface(surface) => self.surface_request(client, asker.view, &surface, now),
            Command::View(open) => self.view_request(client, &open, area),
            Command::ViewSet(set) => self.view_set_asked(asker, &set, area),
            Command::Search(search) => self.search_request(client, &search),
            Command::ConfigPair(pair) => self.config_pair_request(client, &pair, now),
            other => return ControlFlow::Continue(other),
        })
    }

    fn surface_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, area, now, .. } = asked;
        ControlFlow::Break(match command {
            Command::Input(input) => self.input_request(&input),
            Command::PasteFile(paste) => self.paste_file_request(client, paste),
            Command::Scroll(scroll) => self.scroll_request(&scroll),
            Command::Selection(selection) => self.selection_request(&selection),
            Command::Resize(resize) => self.resize_surface_request(&resize, now),
            Command::Redraw(redraw) => self.redraw_surface_request(&redraw, now),
            Command::Ratio(ratio) => self.ratio_request(&ratio, area),
            Command::Link(at) => self.link_request(&at),
            other => return ControlFlow::Continue(other),
        })
    }

    fn changes_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, now, .. } = asked;
        ControlFlow::Break(match command {
            Command::Changes(changes) => self.changes_request(client, asker, &changes),
            Command::ChangesMode(mode) => self.changes_mode_request(client, &mode),
            Command::ChangesFile(file) => self.changes_file_request(client, asker, &file, now),
            Command::ChangesGap(gap) => self.changes_gap_request(client, asker, &gap, now),
            Command::ChangesBase(base) => self.changes_base_request(client, asker, &base),
            Command::ChangesViewed(viewed) => self.changes_viewed_request(asker, &viewed),
            Command::ChangesDiscard(discard) => self.changes_discard_request(client, asker, &discard),
            Command::ChangesStage(stage) => self.changes_stage_request(client, asker, &stage),
            Command::ChangesMergeCheck(check) => self.changes_merge_check_request(client, asker, &check),
            Command::PullRequest(read) => self.pull_request_request(client, asker, &read, now),
            Command::Context(read) => self.context_request(client, asker, &read),
            Command::Recap(ask) => self.recap_request(client, asker, &ask),
            other => return ControlFlow::Continue(other),
        })
    }

    fn issue_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, .. } = asked;
        ControlFlow::Break(match command {
            Command::IssueSources(sources) => self.issue_sources_request(asker, &sources),
            Command::IssueList(list) => self.issue_list_request(client, asker, &list),
            Command::IssueRead(read) => self.issue_read_request(client, asker, &read),
            Command::IssuePeople(people) => self.issue_people_request(client, asker, &people),
            Command::Queue(queue) => self.queue_request(asked, queue),
            other => return ControlFlow::Continue(other),
        })
    }

    fn flow_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, .. } = asked;
        let caller = asker.caller;
        ControlFlow::Break(match command {
            Command::FlowList(list) => self.flow_list_request(caller, &list),
            Command::FlowRun(run) => self.flow_run_request(asked, run),
            Command::FlowExport(export) => self.flow_export_request(caller, &export),
            Command::FlowImport(import) => self.flow_import_request(caller, &import),
            Command::FlowSave(save) => self.flow_save_request(caller, &save),
            Command::FlowDelete(delete) => self.flow_delete_request(caller, &delete),
            Command::FlowHide(hide) => self.flow_hide_request(&hide),
            Command::FlowRestore(restore) => self.flow_restore_request(&restore),
            Command::FlowDefault(req) => self.flow_default_request(caller, &req),
            Command::FlowOrder(req) => self.flow_order_request(caller, &req),
            Command::FlowMessages(messages) => self.flow_messages_request(client, caller, &messages),
            Command::FlowFiles(files) => self.flow_files_request(client, caller, &files),
            other => return ControlFlow::Continue(other),
        })
    }

    fn account_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::AccountList(list) => Ok(self.account_list_request(client, &list)),
            Command::AccountAdd(add) => self.account_add_request(asker, &add, area, now),
            Command::AccountRemove(remove) => self.account_remove_request(&remove),
            Command::AccountRename(rename) => self.account_rename_request(&rename),
            Command::AccountUse(chosen) => self.account_use_request(&chosen),
            Command::AccountRestart(restart) => self.account_restart_request(client, &restart),
            Command::ProfileList(_) => Ok(Some(json(&self.accounts_info(None)))),
            Command::ProfileNew(new) => self.profile_new_request(&new),
            Command::ProfileSet(set) => self.profile_set_request(&set),
            Command::ProfileUse(wanted) => self.profile_use_request(&wanted),
            Command::ProfileDelete(delete) => self.profile_delete_request(&delete),
            other => return ControlFlow::Continue(other),
        })
    }

    fn server_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, now, .. } = asked;
        ControlFlow::Break(match command {
            Command::Config(config) => self.config_request(client, config),
            Command::ConfigExport(_) => self.config_export_request(),
            Command::ConfigImport(import) => self.config_import_request(&import),
            Command::Theme(theme) => self.theme_request(&theme),
            Command::Go(go) => self.go_request(asker, &go),
            Command::UpdateCheck(_) => {
                self.update_check_request(client, now);
                Ok(None)
            }
            Command::UpdateInstall(_) => self.update_install_request(client),
            Command::RestartWhenIdle(restart) => self.restart_request(client, asker.caller, &restart, now),
            Command::StartServer(start) => self.start_server_request(client, &start),
            Command::Away(away) => self.away_request(away.off).map_err(Refusal::from),
            Command::ForumServer(pick) => self.forum_server_request(&pick),
            Command::Usage(usage) => self.usage_request(client, &usage, now),
            Command::Ports(ports) => self.ports_request(client, asker, &ports),
            Command::StopPort(stop) => {
                self.stop_port_request(client, &stop);
                Ok(None)
            }
            Command::Processes(processes) => self.processes_request(client, &processes),
            Command::StopProcess(stop) => {
                self.stop_process_request(client, &stop);
                Ok(None)
            }
            Command::WorktreeSize(size) => self.worktree_size_request(client, asker, &size),
            Command::FreeBuilds(free) => self.free_builds_request(client, asker, &free),
            Command::Worktrees(list) => self.worktrees_request(client, &list),
            Command::WorktreesRemove(remove) => self.worktrees_remove_request(client, &remove),
            Command::Machines(machines) => self.machines_request(client, machines),
            Command::Tour(tour) => self.tour_request(&tour),
            Command::PluginList(_) => Ok(Some(self.plugin_list_request(now))),
            Command::PluginActions(actions) => Ok(Some(self.plugin_actions_request(&actions))),
            Command::PluginRun(run) => self.plugin_run_request(client, asker.view, &run),
            Command::PluginPopupClose(_) => self.plugin_popup_close_request(),
            other => return ControlFlow::Continue(other),
        })
    }

    fn status_value(&self, asker: Asker) -> Value {
        match asker.view.and_then(|view| self.view_report(view)) {
            Some(report) => json(&report),
            None => json(&self.report(asker.caller)),
        }
    }

    fn view_set_asked(&mut self, asker: Asker, set: &control::SetView, area: Option<Rect>) -> Handled {
        match asker.view {
            Some(view) => self.view_set_request(view, set, area),
            None => Err(malformed("`view-set` needs the view to change: pass --view")),
        }
    }

    fn check_view(&self, request: &Request) -> Result<(), Refusal> {
        let Some(view) = request.view else { return Ok(()) };
        if !control::view::takes_view(&request.command) {
            let message = format!("`{}` takes no view", request.command.name());
            return Err(Refusal::new(Code::Malformed, message));
        }
        relay::viewless(&request.command)?;
        if self.clients.views.get(view).is_none() {
            let message = format!(
                "there is no view {view}: it closed with the connection that opened it, or the server restarted"
            );
            return Err(Refusal::new(Code::NotFound, message));
        }
        Ok(())
    }
}
