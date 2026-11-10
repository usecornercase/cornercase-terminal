use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::agents::activity::{Activity, Status};
use crate::app::machine_links::RemoteItem;
use crate::app::tests::close_agent::agent;
use crate::flows;

pub(in crate::app::tests::machine_links) const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 40 };

type Row = (String, Option<String>, usize);

fn pane(id: u64, agent: &str, parent: Option<u64>) -> PaneInfo {
    PaneInfo { id, agent: Some(agent.into()), status: Some("waiting".into()), parent, ..PaneInfo::default() }
}

fn holding(shop: Vec<PaneInfo>, blog: Vec<PaneInfo>) -> control::Report {
    let mut sent = worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]));
    sent.projects[0].workspaces[0].tabs[0].panes = shop;
    sent.projects[1].workspaces[0].tabs[0].panes = blog;
    sent
}

fn arrived(app: &App, at: usize, wanted: &[ProjectInfo]) -> bool {
    app.machines.held()[at].seen.as_ref().is_some_and(|seen| seen.projects == wanted)
}

fn sending(seeing: &mut Seeing, shop: Vec<PaneInfo>, blog: Vec<PaneInfo>) {
    let sent = holding(shop, blog);
    let wanted = sent.projects.clone();
    serve(&mut seeing.theirs, 2, sent);
    seeing.rig.until("the agents arrive", |app| arrived(app, 0, &wanted));
}

fn seeing_agents(shop: Vec<PaneInfo>, blog: Vec<PaneInfo>) -> Seeing {
    let mut seeing = seeing_two();
    sending(&mut seeing, shop, blog);
    seeing
}

fn rows(app: &App) -> Vec<Row> {
    let entries = app.agents_view().entries.into_iter();
    entries.map(|entry| (entry.agent, entry.machine.map(|machine| machine.name), entry.depth)).collect()
}

fn row(agent: &str, machine: Option<&str>, depth: usize) -> Row {
    (agent.into(), machine.map(str::to_string), depth)
}

fn codex_in_a_second_project(rig: &mut Rig) -> TempDir {
    let dir = TempDir::new();
    rig.app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
    let term = rig.app.projects[1].workspaces[0].tabs[0].pane_mut().expect("a pane");
    term.agent.follow(Some(Known::Codex.name()));
    term.agent.update(Some(Activity::Working), true, Instant::now());
    dir
}

pub(in crate::app::tests::machine_links) fn section(app: &mut App) -> String {
    app.config.agents_section = true;
    let terminal = render(app, TALL);
    let list = app.layout(TALL).agents_list;
    let buffer = terminal.backend().buffer();
    let cells = (list.y..list.bottom()).flat_map(|y| (list.x..list.right()).map(move |x| (x, y)));
    cells.map(|cell| buffer[cell].symbol().to_string()).collect()
}

pub(in crate::app::tests::machine_links) fn row_at(app: &App, area: Rect, i: usize) -> Rect {
    let a = app.layout(area).shown(app.screen.nav);
    ui::agent_row(a.agents_list, a.pitch, app.agents_view().entries.len(), 0, i)
}

fn press_row(app: &mut App, i: usize, x: impl Fn(Rect) -> u16) {
    section(app);
    let r = row_at(app, TALL, i);
    click_in(app, Position::new(x(r), r.y), TALL);
}

fn click_row(app: &mut App, i: usize) {
    press_row(app, i, |r| r.x + 4);
}

fn actives(app: &App) -> Vec<bool> {
    app.agents_view().entries.into_iter().map(|entry| entry.active).collect()
}

mod listed {
    use super::*;

    #[test]
    fn only_a_machine_s_panes_with_a_state_make_rows() {
        let (mut rig, ends) = watched();
        let _theirs = linked(&ends);
        rig.until("the machine links", |app| app.machines.held()[0].status.state == State::Linked);
        assert_eq!(rows(&rig.app), []);

        let Seeing { rig, .. } = seeing_two();

        assert_eq!(rows(&rig.app), []);
    }

    #[test]
    fn follow_the_local_ones_of_their_project_and_the_unpaired_ones_come_last() {
        let mut seeing = seeing_agents(vec![pane(12, "gemini", None)], vec![pane(22, "grok", None)]);
        agent(&mut seeing.rig.app, 0, 0);
        let _dir = codex_in_a_second_project(&mut seeing.rig);

        let listed = rows(&seeing.rig.app);

        let wanted =
            [row("claude", None, 0), row("gemini", Some("vps"), 0), row("codex", None, 0), row("grok", Some("vps"), 0)];
        assert_eq!(listed, wanted);
    }

    #[test]
    fn of_a_closing_local_project_come_last() {
        let mut seeing = seeing_agents(vec![pane(12, "gemini", None)], vec![pane(22, "grok", None)]);
        agent(&mut seeing.rig.app, 0, 0);
        seeing.rig.app.projects[0].closing = true;

        let wanted = [row("claude", None, 0), row("gemini", Some("vps"), 0), row("grok", Some("vps"), 0)];
        assert_eq!(rows(&seeing.rig.app), wanted);
        let projects: Vec<String> = seeing.rig.app.agents_view().entries.into_iter().map(|e| e.project).collect();
        assert_eq!(projects[1..], ["web-shop", "blog"]);
    }

    #[test]
    fn show_the_local_project_they_pair_with_and_what_the_machine_says_of_them() {
        let mut working =
            PaneInfo { model: Some("opus".into()), context: Some(40), subagents: 2, ..pane(12, "gemini", None) };
        working.status = Some("working".into());
        working.memory = Some(1 << 30);
        let Seeing { rig, .. } = seeing_agents(vec![working], Vec::new());

        let entry = rig.app.agents_view().entries.remove(0);

        let label = rig.app.project_label(&rig.app.projects[0]);
        let details = (entry.details.model.as_deref(), entry.details.percent, entry.details.subagents);
        assert_eq!((entry.status, entry.project, entry.workspace), (Some(Status::Working), label, None));
        assert_eq!((details, entry.details.memory, entry.active), ((Some("opus"), Some(40), 2), None, false));
    }

    #[test]
    fn name_their_workspace_when_the_machine_has_more_than_one() {
        let mut seeing = seeing_two();
        let mut sent = holding(Vec::new(), vec![pane(22, "grok", None)]);
        let mut other = sent.projects[1].workspaces[0].clone();
        other.id += 5;
        other.name = "fix-feed".into();
        other.tabs[0].panes = vec![pane(26, "codex", None)];
        sent.projects[1].workspaces.push(other);
        let wanted = sent.projects.clone();
        serve(&mut seeing.theirs, 2, sent);
        seeing.rig.until("the agents arrive", |app| arrived(app, 0, &wanted));

        let entries = seeing.rig.app.agents_view().entries;

        let places: Vec<_> = entries.into_iter().map(|e| (e.project, e.workspace)).collect();
        assert_eq!(places, [("blog".into(), Some("main".into())), ("blog".into(), Some("fix-feed".into()))]);
    }

    #[test]
    fn of_two_machines_paired_with_one_project_follow_the_config_s_order() {
        let ends = Ends::default();
        let both = vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")];
        let mut rig = Rig::new(both, There::Watched(Arc::clone(&ends)));
        let dir = TempDir::new();
        rig.app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
        rig.app.projects[0].repo.identity = Some(SHOP.into());
        wait_until("both links connect", || ends.lock().len() == 2);
        let mut sent = holding(vec![pane(12, "gemini", None), pane(13, "codex", Some(12))], Vec::new());
        let wanted = sent.projects.clone();
        let mut streams: Vec<UnixStream> = ends.lock().drain(..).collect();
        for (theirs, boot) in streams.iter_mut().zip(["a-1", "b-1"]) {
            answer(theirs, boot, version::CURRENT);
            request(theirs);
            sent.boot = boot.into();
            serve(theirs, 1, sent.clone());
        }
        rig.until("both snapshots arrive", |app| arrived(app, 0, &wanted) && arrived(app, 1, &wanted));

        let wanted = [
            row("gemini", Some("vps"), 0),
            row("codex", Some("vps"), 1),
            row("gemini", Some("pi"), 0),
            row("codex", Some("pi"), 1),
        ];
        assert_eq!(rows(&rig.app), wanted);
    }

    #[test]
    fn whose_ids_clash_with_local_ones_neither_hang_nor_fold_under_them() {
        let mut seeing = seeing_two();
        let ours = agent(&mut seeing.rig.app, 0, 0);
        seeing.rig.app.projects.pane_by_mut(ours).expect("our agent").set_collapsed(true);
        let folded = PaneInfo { collapsed: true, ..pane(ours, "gemini", None) };
        sending(&mut seeing, vec![folded, pane(ours + 50, "codex", Some(ours))], Vec::new());

        let entries = seeing.rig.app.agents_view().entries;

        let folds: Vec<_> = entries.iter().map(|e| (e.agent.as_str(), e.fold, e.hidden, e.depth)).collect();
        assert_eq!(folds, [("claude", None, 0, 0), ("gemini", Some(true), 1, 0)]);
        assert_eq!(entries[1].attention, Some(Status::Waiting));
    }

    #[test]
    fn hang_a_child_under_its_parent_there_with_its_guide() {
        let Seeing { rig, .. } = seeing_agents(vec![pane(12, "gemini", None), pane(13, "codex", Some(12))], Vec::new());

        let entries = rig.app.agents_view().entries;

        let tree: Vec<_> = entries.iter().map(|e| (e.depth, e.last, e.fold)).collect();
        assert_eq!(tree, [(0, true, Some(false)), (1, true, None)]);
    }

    #[test]
    fn show_a_flow_member_under_its_orchestrator_by_its_role_alone() {
        let lead = PaneInfo { role: Some(flows::ORCHESTRATOR.into()), ..pane(12, "gemini", None) };
        let coder = PaneInfo { role: Some("coder".into()), ..pane(13, "codex", Some(12)) };
        let Seeing { rig, .. } = seeing_agents(vec![lead, coder], Vec::new());

        let entries = rig.app.agents_view().entries;

        let seats: Vec<_> = entries.into_iter().map(|e| (e.role, e.place)).collect();
        assert_eq!(seats, [(Some(flows::ORCHESTRATOR.into()), true), (Some("coder".into()), false)]);
    }

    #[test]
    fn of_a_fallen_machine_stay_dim_with_when_it_was_seen() {
        let Seeing { mut rig, theirs, .. } = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());
        drop(theirs);
        rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);

        let entries = rig.app.agents_view().entries;

        let tag = entries[0].machine.clone().expect("the machine's tag");
        assert!(tag.dim && tag.seen.is_some(), "{tag:?}");
        assert_eq!(entries.len(), 1);
    }
}

mod the_section {
    use super::*;

    #[test]
    fn says_no_agents_running_only_without_any_here_or_there() {
        let mut seeing = seeing_two();
        assert!(section(&mut seeing.rig.app).contains("no agents running"));

        sending(&mut seeing, Vec::new(), vec![pane(22, "grok", None)]);

        let drawn = section(&mut seeing.rig.app);
        assert!(!drawn.contains("no agents running") && drawn.contains("blog"), "{drawn}");
    }

    #[test]
    fn counts_only_the_rows_a_fold_leaves() {
        let parent = PaneInfo { collapsed: true, ..pane(12, "gemini", None) };
        let Seeing { mut rig, .. } = seeing_agents(vec![parent, pane(13, "codex", Some(12))], Vec::new());
        section(&mut rig.app);

        let layout = rig.app.agent_layout(&rig.app.layout(TALL));

        assert_eq!((layout.hidden().1.end, rig.app.agents_view().entries.len()), (1, 1));
    }

    fn family(collapsed: bool, children: u64) -> Vec<PaneInfo> {
        let parent = PaneInfo { collapsed, ..pane(12, "gemini", None) };
        std::iter::once(parent).chain((13..13 + children).map(|id| pane(id, "codex", Some(12)))).collect()
    }

    fn scrolled_down() -> (Seeing, usize) {
        let mut seeing = seeing_agents(family(false, 8), Vec::new());
        seeing.rig.app.screen.widths.agents = Some(6);
        section(&mut seeing.rig.app);
        let app = &mut seeing.rig.app;
        app.scroll.set(List::Agents, app.agent_layout(&app.layout(TALL)).scrolled(100));
        let high = app.scroll.of(List::Agents);
        assert!(high > 0);
        (seeing, high)
    }

    #[rstest]
    #[case::folded_there(true, 8)]
    #[case::finished_there(false, 0)]
    fn keeps_its_scroll_in_range_when_a_snapshot_leaves_fewer_rows(#[case] collapsed: bool, #[case] kept: u64) {
        let (mut seeing, high) = scrolled_down();

        sending(&mut seeing, family(collapsed, kept), Vec::new());

        let app = &seeing.rig.app;
        assert_eq!(app.scroll.of(List::Agents), app.agent_layout(&app.screen.drawn).scrolled(0));
        assert!(app.scroll.of(List::Agents) < high);
    }

    #[test]
    fn leaves_its_scroll_alone_while_switched_off() {
        let (mut seeing, high) = scrolled_down();
        seeing.rig.app.config.agents_section = false;

        sending(&mut seeing, family(false, 0), Vec::new());

        assert_eq!(seeing.rig.app.scroll.of(List::Agents), high);
    }

    #[test]
    fn stays_out_of_sight_while_switched_off() {
        let Seeing { rig, .. } = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());

        let off = rig.app.lists_view(TALL, Instant::now()).agents.is_none();

        assert!(off && rig.app.layout(TALL).agents_list.is_empty());
    }
}

mod clicked {
    use super::*;

    #[test]
    fn shows_the_machine_s_tab_and_its_card_without_starting_another_link() {
        let Seeing { mut rig, .. } = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());
        rig.app.panels.files.set_full_screen(true);

        click_row(&mut rig.app, 0);

        let card = rig.app.remote_card(Instant::now()).map(|card| (card.machine, card.down));
        assert_eq!((picked(&rig.app), card), (Some((10, Some(11))), Some(("vps".into(), None))));
        assert!(!rig.app.panels.files.full_screen());
        assert_eq!(rig.seen("start vps.lan"), 1);
    }

    #[test]
    fn shows_the_tab_of_a_fallen_machine_on_its_card() {
        let Seeing { mut rig, theirs, .. } = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());
        drop(theirs);
        rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);

        click_row(&mut rig.app, 0);

        let down = rig.app.remote_card(Instant::now()).and_then(|card| card.down);
        assert_eq!(picked(&rig.app), Some((10, Some(11))));
        assert!(down.is_some(), "{down:?}");
    }

    #[test]
    fn shows_nothing_once_the_machine_restarted_between_the_press_and_the_release() {
        let mut seeing = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());
        section(&mut seeing.rig.app);
        let r = row_at(&seeing.rig.app, TALL, 0);
        let at = Position::new(r.x + 4, r.y);
        mouse_in(&mut seeing.rig.app, MouseEventKind::Down(MouseButton::Left), at, TALL);
        let restarted = control::Report { boot: "vps-2".into(), ..holding(vec![pane(12, "gemini", None)], Vec::new()) };
        serve(&mut seeing.theirs, 3, restarted);
        seeing.rig.until("the machine restarted", |app| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.boot == "vps-2")
        });

        mouse_in(&mut seeing.rig.app, MouseEventKind::Up(MouseButton::Left), at, TALL);

        assert_eq!((picked(&seeing.rig.app), rows(&seeing.rig.app)), (None, vec![row("gemini", Some("vps"), 0)]));
    }

    #[test]
    fn where_a_local_row_has_its_close_button_shows_the_tab() {
        let Seeing { mut rig, .. } = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());
        section(&mut rig.app);
        let close = ui::agent_close(row_at(&rig.app, TALL, 0), rig.app.layout(TALL).pitch);

        press_row(&mut rig.app, 0, |_| close.x);

        assert_eq!((picked(&rig.app), rig.app.overlay.is_none()), (Some((10, Some(11))), true));
    }

    #[test]
    fn in_compact_mode_closes_the_menu() {
        const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };
        let Seeing { mut rig, .. } = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());
        rig.app.config.agents_section = true;
        rig.app.screen.nav = Some(ui::Nav::Agents);
        render(&mut rig.app, SMALL);
        let r = row_at(&rig.app, SMALL, 0);

        click_in(&mut rig.app, Position::new(r.x + 4, r.y), SMALL);

        assert_eq!((picked(&rig.app), rig.app.screen.nav), (Some((10, Some(11))), None));
    }

    #[test]
    fn then_a_local_row_goes_back_to_what_is_here() {
        let mut seeing = seeing_agents(vec![pane(12, "gemini", None)], Vec::new());
        let ours = agent(&mut seeing.rig.app, 0, 0);
        click_row(&mut seeing.rig.app, 1);
        assert!(picked(&seeing.rig.app).is_some());

        click_row(&mut seeing.rig.app, 0);

        assert_eq!((picked(&seeing.rig.app), seeing.rig.app.term().map(|term| term.id)), (None, Some(ours)));
    }
}

mod the_rail {
    use super::*;

    #[test]
    fn marks_the_row_of_the_pane_active_on_the_tab_shown_and_no_local_one() {
        let lead = PaneInfo { active: true, ..pane(12, "gemini", None) };
        let mut seeing = seeing_agents(vec![lead, pane(13, "codex", None)], Vec::new());
        agent(&mut seeing.rig.app, 0, 0);
        assert_eq!(actives(&seeing.rig.app), [true, false, false]);

        click_row(&mut seeing.rig.app, 2);

        assert_eq!(actives(&seeing.rig.app), [false, true, false]);
    }

    #[test]
    fn marks_the_nearest_ancestor_shown_of_a_folded_active_pane() {
        let parent = PaneInfo { collapsed: true, ..pane(12, "gemini", None) };
        let child = PaneInfo { active: true, ..pane(13, "codex", Some(12)) };
        let Seeing { mut rig, .. } = seeing_agents(vec![parent, child], Vec::new());

        click_row(&mut rig.app, 0);

        assert_eq!(actives(&rig.app), [true]);
    }

    #[test]
    fn leaves_a_machine_s_rows_unmarked_while_another_tab_shows() {
        let lead = PaneInfo { active: true, ..pane(12, "gemini", None) };
        let Seeing { mut rig, .. } = seeing_agents(vec![lead], vec![pane(22, "grok", None)]);

        assert!(rig.app.select_remote("vps", RemoteItem::Tab(13)));

        assert_eq!(actives(&rig.app), [false, false]);
    }
}
