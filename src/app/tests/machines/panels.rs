use rstest::rstest;
use serde::Serialize;
use serde_json::{Value, json};

use super::*;
use crate::app::layout::Side;
use crate::app::machines::RemoteItem;
use crate::control::{ProjectInfo, Response, WorkspaceInfo};
use crate::machines::follow::{Event as Heard, Followed};
use crate::machines::link::fake::{ended, request, respond, serve};

const BOOT: &str = "vps-1";
const SHOP: u64 = 10;
const BLOG: u64 = 20;

fn workspace(id: u64, name: &str) -> WorkspaceInfo {
    let path = format!("/home/ana/{name}").into();
    WorkspaceInfo { id, name: name.into(), path, branch: Some("main".into()), ..WorkspaceInfo::default() }
}

fn holding() -> control::Report {
    let project = ProjectInfo {
        id: 1,
        name: "web".into(),
        path: "/home/ana/web".into(),
        workspaces: vec![workspace(SHOP, "web-shop"), workspace(BLOG, "web-blog")],
        ..ProjectInfo::default()
    };
    let home = Some("/home/ana".into());
    control::Report { boot: BOOT.into(), home, projects: vec![project], ..control::Report::default() }
}

fn tell(stream: &mut UnixStream, value: &impl Serialize) {
    respond(stream, &Response::Ok(serde_json::to_value(value).expect("a JSON answer")));
}

fn taken(ends: &Ends) -> UnixStream {
    wait_until("a connection to the machine", || !ends.lock().is_empty());
    let end = ends.lock().remove(0);
    end.set_read_timeout(Some(Duration::from_secs(10))).expect("a read timeout");
    end
}

struct Panels {
    rig: Rig,
    ends: Ends,
    link: UnixStream,
    clock: u64,
}

impl Panels {
    fn linked() -> Self {
        let ends = Ends::default();
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Watched(Arc::clone(&ends)));
        let mut link = taken(&ends);
        answer(&mut link, BOOT, version::CURRENT);
        request(&mut link);
        serve(&mut link, 1, holding());
        rig.until("the snapshot arrives", |app| app.machines.held()[0].seen.is_some());
        rig.app.window_attached();
        Self { rig, ends, link, clock: 0 }
    }

    fn showing(side: Side) -> (Self, UnixStream) {
        let mut panels = Self::linked();
        panels.pick(SHOP);
        panels.rig.app.show_side(Some(side));
        let stream = panels.opened();
        (panels, stream)
    }

    fn pick(&mut self, workspace: u64) {
        assert!(self.rig.app.select_remote("vps", RemoteItem::Workspace(workspace)));
    }

    fn later(&mut self) -> Duration {
        self.clock += 2;
        Duration::from_secs(self.clock)
    }

    fn sync(&mut self) {
        let after = self.later();
        self.rig.refresh(after);
    }

    fn opened(&mut self) -> UnixStream {
        self.sync();
        taken(&self.ends)
    }

    fn streams(&self) -> usize {
        self.rig.seen("start vps.lan") - 1
    }

    fn down(&self) -> Option<String> {
        down(&self.rig.app)
    }

    fn fall(&mut self) {
        let _ = self.link.shutdown(std::net::Shutdown::Both);
        self.rig.until("the machine falls", |app| app.machines.held()[0].status.state != State::Linked);
        self.sync();
    }

    fn back(&mut self) {
        self.link = taken(&self.ends);
        answer(&mut self.link, BOOT, version::CURRENT);
        request(&mut self.link);
        serve(&mut self.link, 1, holding());
        self.rig.until("the machine is back", |app| app.machines.held()[0].status.state == State::Linked);
    }
}

fn down(app: &App) -> Option<String> {
    let note = match app.screen.side {
        Some(Side::Files) => app.files_view().and_then(|view| view.remote),
        _ => app.panel_view().and_then(|view| view.remote),
    };
    note.and_then(|note| note.down)
}

fn asked(stream: &mut UnixStream) -> Value {
    let asked = request(stream);
    json!({ "command": asked["command"], "boot": asked["boot"], "args": asked["args"] })
}

fn local(panels: &mut Panels) {
    panels.rig.app.leave_remote();
}

fn another_workspace(panels: &mut Panels) {
    panels.pick(BLOG);
}

fn closed(panels: &mut Panels) {
    panels.rig.app.close_panel();
}

fn files(panels: &mut Panels) {
    panels.rig.app.show_side(Some(Side::Files));
}

fn window_gone(panels: &mut Panels) {
    panels.rig.app.window_gone();
}

fn removed(panels: &mut Panels) {
    let after = panels.later();
    panels.rig.edit(Vec::new(), after);
}

fn stopped(panels: &mut Panels) {
    panels.rig.app.stop_background();
}

mod a_panel_stream {
    use super::*;

    #[test]
    fn follows_the_changes_of_the_chosen_workspace_while_changes_shows() {
        let (_panels, mut stream) = Panels::showing(Side::Changes);

        let args = json!({ "workspace": SHOP, "scope": "project", "mode": "uncommitted" });
        assert_eq!(asked(&mut stream), json!({ "command": "changes", "boot": BOOT, "args": args }));
    }

    #[test]
    fn follows_the_tree_of_the_chosen_workspace_while_files_shows() {
        let (_panels, mut stream) = Panels::showing(Side::Files);

        let args = json!({ "workspace": SHOP, "folders": [""], "follow": true });
        assert_eq!(asked(&mut stream), json!({ "command": "files-list", "boot": BOOT, "args": args }));
    }

    #[rstest]
    #[case::when_something_of_this_computer_is_chosen(local)]
    #[case::when_another_workspace_of_the_machine_is_chosen(another_workspace)]
    #[case::when_the_panel_closes(closed)]
    #[case::when_the_other_panel_takes_its_place(files)]
    #[case::when_the_last_window_goes(window_gone)]
    #[case::when_the_machine_is_removed(removed)]
    #[case::when_the_server_stops(stopped)]
    fn ends(#[case] leave: fn(&mut Panels)) {
        let (mut panels, mut stream) = Panels::showing(Side::Changes);
        request(&mut stream);

        leave(&mut panels);
        panels.sync();

        assert!(ended(&mut stream));
    }

    #[test]
    fn opens_again_when_the_workspace_is_chosen_again() {
        let (mut panels, mut stream) = Panels::showing(Side::Changes);
        request(&mut stream);
        panels.rig.app.leave_remote();
        panels.sync();
        assert!(ended(&mut stream));

        panels.pick(SHOP);
        let mut again = panels.opened();

        assert_eq!(asked(&mut again)["command"], "changes");
        assert_eq!(panels.streams(), 2);
    }

    #[test]
    fn moves_to_the_other_workspace_chosen() {
        let (mut panels, mut stream) = Panels::showing(Side::Changes);
        request(&mut stream);

        panels.pick(BLOG);
        let mut moved = panels.opened();

        assert_eq!(asked(&mut moved)["args"]["workspace"], BLOG);
        assert!(ended(&mut stream));
    }

    #[test]
    fn opens_only_with_a_window_and_once_one_is_back() {
        let mut panels = Panels::linked();
        panels.rig.app.window_gone();
        panels.pick(SHOP);
        panels.rig.app.show_side(Some(Side::Changes));

        panels.sync();
        assert_eq!(panels.streams(), 0);

        panels.rig.app.window_attached();
        let mut stream = panels.opened();
        assert_eq!(asked(&mut stream)["command"], "changes");
    }

    #[test]
    fn opens_nothing_while_the_machine_is_down() {
        let mut panels = Panels::linked();
        let _ = panels.link.shutdown(std::net::Shutdown::Both);
        panels.rig.until("the machine falls", |app| app.machines.held()[0].status.state != State::Linked);
        panels.pick(SHOP);
        panels.rig.app.show_side(Some(Side::Changes));

        panels.sync();

        assert_eq!(panels.streams(), 0);
    }

    #[test]
    fn that_the_machine_ends_says_so_and_is_opened_again_after_a_wait_that_wakes_the_server() {
        let (mut panels, stream) = Panels::showing(Side::Changes);
        drop(stream);
        panels.rig.until("the stream ends", |app| down(app).is_some());

        panels.sync();
        let at = panels.rig.start + Duration::from_secs(panels.clock);
        let wake = panels.rig.app.card_wake(at + Duration::from_millis(300));
        assert_eq!((panels.streams(), wake), (1, Some(Duration::from_millis(700))));

        let mut again = panels.opened();
        assert_eq!(asked(&mut again)["command"], "changes");
        tell(&mut again, &control::changes::Summary { boot: BOOT.into(), ..control::changes::Summary::default() });
        panels.rig.until("the note goes", |app| down(app).is_none());
        assert_eq!(panels.streams(), 2);
    }

    #[test]
    fn that_ends_again_waits_longer() {
        let (mut panels, stream) = Panels::showing(Side::Changes);
        drop(stream);
        panels.rig.until("the stream ends", |app| down(app).is_some());
        panels.sync();
        drop(panels.opened());
        panels.rig.until("it ends again", |app| app.panels_wake(Instant::now()).is_none());

        let at = panels.rig.start + Duration::from_secs(panels.clock + 2);
        panels.sync();
        let wake = panels.rig.app.card_wake(at + Duration::from_millis(1_300));

        assert_eq!((panels.streams(), wake), (2, Some(Duration::from_millis(700))));
    }

    #[test]
    fn ignores_what_comes_from_one_it_does_not_hold() {
        let (mut panels, _stream) = Panels::showing(Side::Changes);

        let stray = Followed { id: 99, event: Heard::Ended("Connection to vps closed.".into()) };
        panels.rig.app.machine_event(MachineEvent::Followed(stray));

        assert_eq!(panels.down(), None);
    }
}

mod changes;
mod files;
