use crate::agents;
use crate::config::{Config, Sidebar, Tabs, WorkingIcon};
use crate::host::notify;
use crate::list::Search;

use super::{Action, EXTRA_ARGS, Edit, NO_PLUGINS, Pick, PickItem, Row, Settings, Status};

impl Settings {
    fn flip(&mut self, switch: fn(&mut Config) -> &mut bool, [on, off]: [&str; 2]) -> Action {
        let mut config = self.config.clone();
        let value = switch(&mut config);
        *value = !*value;
        let notice = if *value { on } else { off };
        self.save(config, notice.into())
    }

    #[expect(clippy::too_many_lines, reason = "one arm per row")]
    pub(crate) fn activate(&mut self) -> Action {
        let Some(row) = self.row() else { return Action::None };
        self.notice = None;
        match row {
            Row::Folder => self.start_edit(row, self.config.worktrees_dir.clone()),
            Row::Fetch => self.start_edit(row, self.config.fetch_minutes.to_string()),
            Row::PullRequests => self.flip(
                |c| &mut c.pull_requests,
                ["workspaces show their pull request and its checks", "workspaces no longer show pull requests"],
            ),
            Row::MergedPullRequests => self.flip(
                |c| &mut c.merged_pull_requests,
                ["Stats counts your merged pull requests", "Stats no longer counts merged pull requests"],
            ),
            Row::Token(source) => {
                if self.status(source) == Some(&Status::Env) {
                    let env = source.token_env().unwrap_or_default();
                    self.notice = Some(format!("{env} is set: change or unset it there"));
                    return Action::None;
                }
                self.start_edit(row, String::new())
            }
            Row::GitlabUrl => self.start_edit(row, self.config.gitlab_url.clone()),
            Row::JiraSite => self.start_edit(row, self.config.jira_site.clone()),
            Row::JiraEmail => self.start_edit(row, self.config.jira_email.clone()),
            Row::JiraJql => self.start_edit(row, self.config.jira_jql.clone()),
            Row::PlaneWorkspace => self.start_edit(row, self.config.plane_workspace.clone()),
            Row::PlaneUrl => self.start_edit(row, self.config.plane_url.clone()),
            Row::PlaneFilter => self.start_edit(row, self.config.plane_filter.clone()),
            Row::AsanaWorkspace => self.start_edit(row, self.config.asana_workspace.clone()),
            Row::AsanaProjects => self.start_edit(row, self.config.asana_projects.clone()),
            Row::Tab(id) => self.toggle_tab(id),
            Row::DefaultAgent => self.pick_default_agent(row),
            Row::Submit => {
                self.flip(|c| &mut c.submit, ["the prompt is sent for you", "the prompt is typed; you press Enter"])
            }
            Row::IconBar => self.flip(
                |c| &mut c.icon_bar,
                ["the buttons are icons on the left", "the buttons are back under the lists"],
            ),
            Row::AgentsSection => self.flip(
                |c| &mut c.agents_section,
                ["the sidebar lists every agent", "the sidebar no longer lists agents"],
            ),
            Row::Counts => {
                self.flip(|c| &mut c.counts, ["rows show how many they hold", "rows no longer show how many they hold"])
            }
            Row::TodoDot => self
                .flip(|c| &mut c.todo_dot, ["TODO shows a dot while items are pending", "TODO no longer shows a dot"]),
            Row::DimPanes => {
                self.flip(|c| &mut c.dim_inactive_panes, ["inactive panes are dimmed", "every pane looks the same"])
            }
            Row::Detail(detail) => {
                let mut config = self.config.clone();
                let on = !detail.on(&config);
                *detail.switch(&mut config) = on;
                self.save(config, detail.notice(on).into())
            }
            Row::Sidebar => self.pick_from(row, Sidebar::choices()),
            Row::Tabs => self.pick_from(row, Tabs::choices()),
            Row::WorkingIcon => self.pick_from(row, WorkingIcon::choices()),
            Row::Notifications => self.pick_from(row, notify::choices()),
            Row::Sound => self.flip(
                |c| &mut c.sound,
                ["a sound plays when an agent needs you or finishes", "agents no longer play a sound"],
            ),
            Row::SoundWaiting => self.start_edit(row, self.config.sound_waiting.clone()),
            Row::SoundDone => self.start_edit(row, self.config.sound_done.clone()),
            Row::SoundSubagents => self.flip(
                |c| &mut c.sound_subagents,
                ["agents started by other agents play a sound too", "only agents you started play a sound"],
            ),
            Row::Updates => self.flip(
                |c| &mut c.check_updates,
                ["cornercase looks for new versions", "cornercase no longer looks for new versions"],
            ),
            Row::StarPrompt => self.flip(
                |c| &mut c.star_prompt,
                ["cornercase may ask for a star on GitHub", "cornercase never asks for a star"],
            ),
            Row::Trust => self.flip(
                |c| &mut c.accept_trust_prompts,
                ["trust prompts are accepted for you", "trust prompts are left to you"],
            ),
            Row::Prefix => self.start_capture(),
            Row::Remote => self.flip(
                |c| &mut c.remote,
                ["remote access is on: click Remote to connect a phone", "remote access is off"],
            ),
            Row::Browsers => Action::ForgetBrowsers,
            Row::Resume => self.flip(
                |c| &mut c.resume_agents,
                ["conversations resume after a restart", "agents no longer resume after a restart"],
            ),
            Row::KeepAwake => self.flip(
                |c| &mut c.keep_awake,
                ["the computer stays awake while an agent works", "the computer sleeps as usual"],
            ),
            Row::Kind(kind) => self.pick_mode(kind),
            Row::Bot(_) => self.start_edit(row, String::new()),
            Row::Chat(via) => Action::Pair(via),
            Row::Plugin(_) | Row::NoPlugins => self.switch_plugin(row),
            Row::ServerTokens | Row::Account(..) | Row::AddAccount(_) | Row::System(_) | Row::RenameAccount(..) => {
                self.activate_account(row)
            }
            Row::AddAgent => {
                let listed = self.listed_kinds();
                let items: Vec<PickItem> = agents::kinds(&self.config)
                    .into_iter()
                    .filter(|k| !listed.contains(k))
                    .map(|kind| PickItem { note: agents::command(&self.config, &kind), value: kind, dangerous: false })
                    .collect();
                if items.is_empty() {
                    self.notice = Some("every known agent is listed already".into());
                } else {
                    self.open_pick(row, items);
                }
                Action::None
            }
        }
    }

    fn switch_plugin(&mut self, row: Row) -> Action {
        let Row::Plugin(id) = row else {
            self.notice = Some(NO_PLUGINS.into());
            return Action::None;
        };
        match self.plugins.iter().find(|plugin| plugin.id == id) {
            Some(plugin) if plugin.broken => {
                self.notice = Some(format!("{} cannot run: {}", plugin.label, plugin.note));
                Action::None
            }
            Some(plugin) => Action::Plugin(id, !plugin.on),
            None => Action::None,
        }
    }

    pub(super) fn agent_items(&self) -> Vec<PickItem> {
        let auto = PickItem {
            value: agents::AUTO.into(),
            note: "the agent in your tab, otherwise ask".into(),
            dangerous: false,
        };
        let mut items = vec![auto];
        items.extend(agents::kinds(&self.config).into_iter().map(|kind| PickItem {
            note: agents::command(&self.config, &kind),
            value: kind,
            dangerous: false,
        }));
        items
    }

    fn pick_default_agent(&mut self, row: Row) -> Action {
        let items = self.agent_items();
        self.open_pick(row, items);
        Action::None
    }

    pub(super) fn mode_items(&self, kind: &str) -> Vec<PickItem> {
        let modes = agents::modes(&self.config, kind);
        let mut items = vec![PickItem { value: "default".into(), note: "no mode arguments".into(), dangerous: false }];
        items.extend(modes.iter().map(|(name, args)| PickItem {
            value: name.clone(),
            note: agents::join_args(args),
            dangerous: agents::is_dangerous(name),
        }));
        items
    }

    fn pick_mode(&mut self, kind: String) -> Action {
        let modes = agents::modes(&self.config, &kind);
        if modes.is_empty() {
            self.edit_extra(&kind);
            return Action::None;
        }
        let mut items = self.mode_items(&kind);
        let extra = agents::extra_args(&kind, &agents::args(&self.config, &kind), &modes);
        items.push(PickItem { value: EXTRA_ARGS.into(), note: agents::join_args(&extra), dangerous: false });
        self.open_pick(Row::Kind(kind), items);
        Action::None
    }

    pub(super) fn start_edit(&mut self, row: Row, input: String) -> Action {
        self.edit = Some(Edit { row, input, error: None });
        Action::None
    }

    pub(super) fn edit_extra(&mut self, kind: &str) {
        let modes = agents::modes(&self.config, kind);
        let extra = agents::extra_args(kind, &agents::args(&self.config, kind), &modes);
        self.start_edit(Row::Kind(kind.into()), agents::join_args(&extra));
    }

    fn pick_from(&mut self, row: Row, choices: Vec<(&'static str, &'static str)>) -> Action {
        let items = choices
            .into_iter()
            .map(|(value, note)| PickItem { value: value.into(), note: note.into(), dangerous: false })
            .collect();
        self.open_pick(row, items);
        Action::None
    }

    pub(super) fn open_pick(&mut self, row: Row, items: Vec<PickItem>) {
        let current = match &row {
            Row::DefaultAgent => Some(self.config.agent.clone()),
            Row::Sidebar => Some(Sidebar::from_setting(&self.config.sidebar).id().to_string()),
            Row::Tabs => Some(Tabs::from_setting(&self.config.tabs).id().to_string()),
            Row::WorkingIcon => Some(WorkingIcon::from_setting(&self.config.working_icon).id().to_string()),
            Row::Notifications => Some(self.config.desktop_notifications.trim().to_lowercase()),
            Row::Kind(kind) => Some(
                agents::mode_of(kind, &agents::args(&self.config, kind), &agents::modes(&self.config, kind))
                    .unwrap_or_else(|| "default".into()),
            ),
            _ => None,
        };
        let mut search = Search::default();
        if let Some(i) = current.and_then(|c| items.iter().position(|item| item.value == c)) {
            search.select(i);
        }
        self.pick = Some(Pick { row, items, search });
    }
}
