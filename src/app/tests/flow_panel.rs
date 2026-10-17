use serde_json::json;

use super::*;
use crate::agents::Known;
use crate::agents::activity::Activity;
use crate::app::flow_panel::FRAME_EVERY;
use crate::control::{self as wire, Command};
use crate::flows::ORCHESTRATOR;
use crate::flows::builtin;
use crate::flows::progress::Edge;
use crate::flows::run::{Kind, Member, Run};
use crate::shortcuts::Group;

const CLIENT: u64 = 7;
const TALL: Rect = Rect { x: 0, y: 0, width: 120, height: 60 };
const ROLES: [&str; 5] = [ORCHESTRATOR, "coder", "reviewer", "security", "tester"];

pub(super) fn team() -> (App, Receiver<AppEvent>, TempDir) {
    let dir = TempDir::new();
    let (mut app, rx) = app_in(dir.path(), no_config());
    let flow = builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0);
    let p = app.active;
    let mut run = Run::new(app.projects[p].workspaces[0].id, flow, "fix the login\nand the rest".into());
    for role in ROLES {
        let t = app.push_tab(p, 0, AREA, Some(role.into())).expect("a tab for the role");
        let pane = app.projects[p].workspaces[0].tabs[t].panes[0].id;
        run.members.push(Member { role: role.into(), pane });
    }
    app.runs.push(run);
    (app, rx, dir)
}

fn pane(app: &App, role: &str) -> u64 {
    app.runs[0].pane_of(role).expect("a member")
}

fn ask(app: &mut App, caller: Option<u64>, command: Command) {
    let server = Some(wire::server_token().to_string());
    let text =
        serde_json::to_string(&wire::Request { caller, server, contract: None, boot: None, view: None, command })
            .expect("json");
    app.request(CLIENT, &text, Some(AREA), Instant::now());
}

fn send(app: &mut App, from: &str, to: &str, text: &str) {
    let send = wire::SendText { pane: Some(pane(app, to)), text: Some(text.into()), ..wire::SendText::default() };
    ask(app, Some(pane(app, from)), Command::Send(send));
}

fn messages(app: &App) -> Vec<(String, String, Kind, Option<String>)> {
    let run = &app.runs[0];
    run.messages.iter().map(|m| (m.from.clone(), m.to.clone(), m.kind, m.text.clone())).collect()
}

fn spot(app: &mut App, text: &str) -> Position {
    let t = render(app, TALL);
    let buf = t.backend().buffer();
    let panel = app.layout(TALL).changes;
    let width = u16::try_from(text.chars().count()).expect("short");
    (panel.y..panel.bottom())
        .find_map(|y| {
            (panel.x..panel.right().saturating_sub(width)).find_map(|x| {
                let found: String = (x..x + width).map(|x| buf[(x, y)].symbol()).collect();
                (found == text).then(|| Position::new(x, y))
            })
        })
        .expect("on screen")
}

mod opening {
    use super::*;

    #[test]
    fn a_workspace_without_a_run_offers_none() {
        let (mut app, _rx) = app();
        app.flow_by_key();
        assert_eq!((app.side, toast(&app)), (None, Some("this workspace runs no flow")));
        app.show_flow();
        assert_eq!(app.side, None);
    }

    #[test]
    fn the_keys_menu_opens_and_closes_it() {
        let (mut app, _rx, _dir) = team();
        app.config.prefix_key = "ctrl+]".into();
        let keys = |app: &mut App| {
            send_key(app, KeyCode::Char(']'), KeyModifiers::CONTROL);
            assert!(matches!(app.overlay, Some(Overlay::Keys(None))));
            send_key(app, KeyCode::Char('w'), KeyModifiers::NONE);
            assert!(matches!(app.overlay, Some(Overlay::Keys(Some(Group::Workspace)))));
            send_key(app, KeyCode::Char('f'), KeyModifiers::NONE);
        };
        keys(&mut app);
        assert_eq!((app.side, app.panel_shown()), (Some(Side::Flow), true));
        keys(&mut app);
        assert_eq!(app.side, None);
    }

    #[test]
    fn the_workspace_menu_shows_it() {
        let (mut app, _rx, _dir) = team();
        let (project, workspace) = (app.projects[0].id, app.runs[0].workspace);
        let menu = app.target_menu(Target::Workspace(project, workspace));
        assert!(menu.contains(&MenuAction::ShowFlow(workspace)), "{menu:?}");
        app.menu_action(MenuAction::ShowFlow(workspace), Position::default(), AREA).expect("the action");
        assert_eq!(app.side, Some(Side::Flow));
    }

    #[test]
    fn a_workspace_without_a_run_has_no_entry_in_its_menu() {
        let (app, _rx) = app();
        let (project, workspace) = (app.projects[0].id, app.projects[0].workspaces[0].id);
        assert!(!app.target_menu(Target::Workspace(project, workspace)).contains(&MenuAction::ShowFlow(workspace)));
    }

    #[test]
    fn the_icon_bar_has_a_slot_while_the_workspace_has_a_run() {
        let (mut app, _rx, _dir) = team();
        app.config.icon_bar = true;
        let bar = app.layout(AREA).activity;
        let slot =
            ui::activity_items(bar, app.activity_shown()).into_iter().find(|(i, _)| *i == ui::ActivityItem::Flow);
        let (_, r) = slot.expect("a flow slot");
        click(&mut app, Position::new(r.x + 1, r.y));
        assert_eq!(app.side, Some(Side::Flow));
        app.runs.clear();
        assert!(!app.activity_shown().flow);
        assert!(!app.panel_shown());
    }

    #[test]
    fn esc_closes_it() {
        let (mut app, _rx, _dir) = team();
        app.show_flow();
        send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.side, None);
    }

    #[test]
    fn its_close_button_closes_it() {
        let (mut app, _rx, _dir) = team();
        app.show_flow();
        let close = ui::flow::close(app.layout(AREA).changes).as_position();
        click(&mut app, close);
        assert_eq!(app.side, None);
    }

    #[test]
    fn it_names_the_flow_and_the_first_line_of_the_task() {
        let (mut app, _rx, _dir) = team();
        app.show_flow();
        let screen = format!("{:?}", render(&mut app, TALL).backend());
        assert!(screen.contains("Full issue") && screen.contains("fix the login"), "{screen}");
        assert!(!screen.contains("and the rest"), "{screen}");
    }
}

mod recording {
    use super::*;

    #[test]
    fn a_send_between_members_of_the_run_is_recorded_with_its_text() {
        let (mut app, _rx, _dir) = team();
        send(&mut app, ORCHESTRATOR, "coder", "fix the login");
        let expected = (ORCHESTRATOR.into(), "coder".into(), Kind::Send, Some("fix the login".into()));
        assert_eq!(messages(&app), vec![expected]);
        assert_eq!(app.runs[0].progress.active, Some(0));
    }

    #[test]
    fn a_request_from_outside_the_run_is_not() {
        let (mut app, _rx, _dir) = team();
        let send = wire::SendText { pane: Some(pane(&app, "coder")), text: Some("hi".into()), ..Default::default() };
        ask(&mut app, None, Command::Send(send));
        assert_eq!(messages(&app), Vec::new());
    }

    #[test]
    fn a_refused_request_is_not() {
        let (mut app, _rx, _dir) = team();
        let send = wire::SendText { pane: Some(pane(&app, "coder")), ..wire::SendText::default() };
        let me = Some(pane(&app, ORCHESTRATOR));
        ask(&mut app, me, Command::Send(send));
        assert_eq!(messages(&app), Vec::new());
    }

    #[test]
    fn keys_reads_and_waits_name_their_targets() {
        let (mut app, _rx, _dir) = team();
        let me = Some(pane(&app, ORCHESTRATOR));
        let keys = wire::Keys { pane: Some(pane(&app, "coder")), keys: vec!["Enter".into()], ..wire::Keys::default() };
        ask(&mut app, me, Command::Keys(keys));
        let read = wire::Read { pane: Some(pane(&app, "tester")), ..wire::Read::default() };
        ask(&mut app, me, Command::Read(read));
        let panes = vec![pane(&app, "reviewer"), pane(&app, "security"), pane(&app, "reviewer")];
        let wait = wire::Wait { panes, until: wire::Until::Shell, ..wire::Wait::default() };
        ask(&mut app, me, Command::WaitSeveral(wait));
        let got: Vec<(String, Kind, Option<String>)> =
            messages(&app).into_iter().map(|(_, to, k, t)| (to, k, t)).collect();
        assert_eq!(
            got,
            vec![
                ("coder".into(), Kind::Keys, Some("Enter".into())),
                ("tester".into(), Kind::Read, None),
                ("reviewer".into(), Kind::Wait, None),
                ("security".into(), Kind::Wait, None),
            ]
        );
        assert_eq!(app.runs[0].progress.active, None);
    }

    #[test]
    fn a_last_message_is_recorded_with_its_text_when_the_answer_comes() {
        let (mut app, _rx, _dir) = team();
        let last = wire::LastMessage { pane: Some(pane(&app, "reviewer")), ..wire::LastMessage::default() };
        let noted = app.flow_steps(Some(pane(&app, ORCHESTRATOR)), &Command::LastMessage(last), Instant::now());
        app.flow_requested(CLIENT, noted, true);
        assert_eq!(messages(&app), Vec::new());
        app.flow_answered(CLIENT + 1, &Ok(json!({ "text": "not this one" })));
        app.flow_answered(CLIENT, &Ok(json!({ "text": "No findings." })));
        let expected = (ORCHESTRATOR.into(), "reviewer".into(), Kind::Read, Some("No findings.".into()));
        assert_eq!(messages(&app), vec![expected]);
    }

    #[rstest::rstest]
    #[case::its_run_went_and_another_started(true)]
    #[case::its_client_left(false)]
    fn a_last_message_read_is_dropped_once(#[case] replaced: bool) {
        let (mut app, _rx, _dir) = team();
        let last = wire::LastMessage { pane: Some(pane(&app, "reviewer")), ..wire::LastMessage::default() };
        let noted = app.flow_steps(Some(pane(&app, ORCHESTRATOR)), &Command::LastMessage(last), Instant::now());
        app.flow_requested(CLIENT, noted, true);
        if replaced {
            let mut next = app.runs.remove(0);
            next.id += 1;
            app.runs.push(next);
        } else {
            app.forget(CLIENT);
        }

        app.flow_answered(CLIENT, &Ok(json!({ "text": "No findings." })));

        assert_eq!(messages(&app), Vec::new());
    }

    #[test]
    fn the_open_flow_panel_comes_back_after_a_restart() {
        let (mut app, _rx, _dir) = team();
        app.show_flow();
        let saved = app.state();
        assert!(saved.flow);
        let (mut again, _rx) = new_app(no_config());

        assert!(again.restore(&saved, AREA));

        assert_eq!((again.runs.len(), again.side), (1, Some(Side::Flow)));
    }

    #[test]
    fn a_last_message_that_fails_is_not() {
        let (mut app, _rx, _dir) = team();
        let last = wire::LastMessage { pane: Some(pane(&app, "reviewer")), ..wire::LastMessage::default() };
        let me = Some(pane(&app, ORCHESTRATOR));
        ask(&mut app, me, Command::LastMessage(last));
        assert_eq!((messages(&app), app.take_answers().len()), (Vec::new(), 1));
    }

    #[test]
    fn long_texts_are_cut_in_memory_but_keep_their_size() {
        let (mut app, _rx, _dir) = team();
        let text = "x".repeat(40 * 1024);
        send(&mut app, ORCHESTRATOR, "coder", &text);
        let message = &app.runs[0].messages[0];
        assert_eq!((message.text.as_ref().map(String::len), message.bytes), (Some(32 * 1024), text.len()));
    }

    #[test]
    fn wide_texts_are_cut_at_32_kib_too() {
        let (mut app, _rx, _dir) = team();
        let text = "é".repeat(20 * 1024);
        send(&mut app, ORCHESTRATOR, "coder", &text);
        let kept = app.runs[0].messages[0].text.clone().expect("the text");
        assert!(kept.len() <= 32 * 1024 && kept.len() > 32 * 1024 - 4, "{}", kept.len());
        assert!(text.starts_with(&kept));
    }
}

mod clicks {
    use super::*;

    #[test]
    fn a_box_jumps_to_its_pane() {
        let (mut app, _rx, _dir) = team();
        app.show_flow();
        let at = spot(&mut app, "reviewer");
        click_in(&mut app, at, TALL);
        assert_eq!(app.tab().and_then(Tab::pane).map(|t| t.id), Some(pane(&app, "reviewer")));
        assert_eq!(app.side, Some(Side::Flow));
    }

    #[test]
    fn an_arrow_shows_the_last_message_sent_along_it_and_copies_it() {
        let (mut app, _rx, _dir) = team();
        send(&mut app, ORCHESTRATOR, "coder", "fix the login");
        send(&mut app, ORCHESTRATOR, "reviewer", "review the fix");
        app.show_flow();
        let at = spot(&mut app, "├▶");
        click_in(&mut app, at, TALL);
        let Some(overlay @ Overlay::FlowMessage { edge, .. }) = &app.overlay else { panic!("no message dialog") };
        assert_eq!(*edge, Edge::Forward(0));
        let Some(ui::Overlay::Update(view)) = app.overlay_view(overlay, TALL) else { panic!("not drawn") };
        let notes: Vec<String> = view.notes.iter().map(ToString::to_string).collect();
        assert_eq!((view.message.as_str(), notes), ("orchestrator → reviewer · sent", vec!["review the fix".into()]));
        send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.take_host_writes(), vec![clipboard::osc52("review the fix")]);
        assert!(app.overlay.is_none());
    }

    #[test]
    fn a_branch_shows_the_message_its_own_role_got() {
        let (mut app, _rx, _dir) = team();
        send(&mut app, ORCHESTRATOR, "coder", "fix the login");
        send(&mut app, ORCHESTRATOR, "reviewer", "review the fix");
        send(&mut app, ORCHESTRATOR, "security", "look for holes");
        app.show_flow();
        let opened = |app: &mut App, branch: &str| {
            let at = spot(app, branch);
            click_in(app, Position { x: at.x + 1, ..at }, TALL);
            let overlay = app.overlay.take().expect("a message dialog");
            let Some(ui::Overlay::Update(view)) = app.overlay_view(&overlay, TALL) else { panic!("not drawn") };
            (view.message, view.notes.iter().map(ToString::to_string).collect::<Vec<_>>())
        };
        assert_eq!(opened(&mut app, "├▶"), ("orchestrator → reviewer · sent".into(), vec!["review the fix".into()]));
        assert_eq!(opened(&mut app, "╰▶"), ("orchestrator → security · sent".into(), vec!["look for holes".into()]));
    }

    #[test]
    fn the_dialog_goes_when_its_run_does() {
        let (mut app, _rx, _dir) = team();
        send(&mut app, ORCHESTRATOR, "coder", "fix the login");
        send(&mut app, ORCHESTRATOR, "reviewer", "review the fix");
        let workspace = app.runs[0].workspace;
        app.overlay = Some(Overlay::FlowMessage { workspace, edge: Edge::Forward(0), to: None, scroll: 0 });
        app.runs.clear();
        app.refresh(Instant::now());
        assert!(app.overlay.is_none());
    }
}

mod animation {
    use super::*;

    #[test]
    fn frames_are_asked_only_while_a_dot_travels_in_a_shown_panel() {
        let (mut app, _rx, _dir) = team();
        send(&mut app, ORCHESTRATOR, "coder", "fix the login");
        send(&mut app, ORCHESTRATOR, "reviewer", "review the fix");
        let now = Instant::now();
        assert_eq!(app.flow_frame(now), None);
        app.show_flow();
        assert_eq!(app.flow_frame(now), Some(FRAME_EVERY));
        assert_eq!(app.flow_frame(now + Duration::from_secs(2)), None);
    }

    #[test]
    fn a_working_role_shows_in_its_colour() {
        let (mut app, _rx, _dir) = team();
        let coder = pane(&app, "coder");
        let term = app.projects.pane_by_mut(coder).expect("the coder");
        term.agent.update(Some(Activity::Working), true, Instant::now());
        app.show_flow();
        let at = spot(&mut app, "coder");
        let t = render(&mut app, TALL);
        assert_eq!(t.backend().buffer()[(at.x - 2, at.y)].symbol(), "━");
    }
}

mod files {
    use std::time::SystemTime;

    use super::*;

    fn screen_text(t: &Terminal<TestBackend>) -> String {
        t.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect()
    }

    fn with_folder(app: &mut App, dir: &TempDir, files: &[(&str, u64)]) -> PathBuf {
        let folder = dir.path().join("run");
        std::fs::create_dir_all(&folder).expect("the run's folder");
        for (name, age) in files {
            let file = std::fs::File::create(folder.join(name)).expect("a file");
            file.set_modified(SystemTime::now() - Duration::from_secs(*age)).expect("a time");
        }
        app.runs[0].dir = Some(folder.clone());
        folder
    }

    fn listed(app: &mut App, rx: &Receiver<AppEvent>, text: &str) {
        wait_until_within(text, Duration::from_secs(10), || {
            drain(app, rx);
            app.refresh(Instant::now());
            let t = render(app, TALL);
            let screen = screen_text(&t);
            screen.contains(text)
        });
    }

    #[test]
    fn the_panel_lists_the_folders_files_newest_first_with_their_ages() {
        let (mut app, rx, dir) = team();
        with_folder(&mut app, &dir, &[("old.md", 7200), ("coder-1.md", 60)]);
        app.show_flow();

        listed(&mut app, &rx, "coder-1.md");

        let (newest, oldest) = (spot(&mut app, "coder-1.md"), spot(&mut app, "old.md"));
        assert!(newest.y < oldest.y);
        assert_eq!(newest.y, spot(&mut app, "Files ·").y + 1);
        assert_eq!(oldest.y, newest.y + 1);
        let t = render(&mut app, TALL);
        let row: String = (newest.x..TALL.right()).map(|x| t.backend().buffer()[(x, oldest.y)].symbol()).collect();
        assert!(row.contains("2h"), "{row}");
    }

    #[test]
    fn a_click_on_a_file_opens_it_in_the_files_panel() {
        let (mut app, rx, dir) = team();
        let folder = with_folder(&mut app, &dir, &[("coder-1.md", 60)]);
        std::fs::write(folder.join("coder-1.md"), "the answer").expect("write");
        app.show_flow();
        listed(&mut app, &rx, "coder-1.md");

        let at = spot(&mut app, "coder-1.md");
        click_in(&mut app, at, TALL);

        assert_eq!(app.side, Some(Side::Files));
        wait_until_within("the file is read", Duration::from_secs(10), || {
            drain(&mut app, &rx);
            app.refresh(Instant::now());
            let t = render(&mut app, TALL);
            let screen = screen_text(&t);
            screen.contains("the answer")
        });
    }

    #[test]
    fn an_empty_folder_says_so() {
        let (mut app, rx, dir) = team();
        with_folder(&mut app, &dir, &[]);
        app.show_flow();
        listed(&mut app, &rx, "nothing yet");
    }

    #[test]
    fn a_run_without_a_folder_shows_no_files_section() {
        let (mut app, _rx, _dir) = team();
        app.show_flow();
        let t = render(&mut app, TALL);
        let screen = screen_text(&t);
        assert!(!screen.contains("Files ·"), "{screen}");
    }
}
