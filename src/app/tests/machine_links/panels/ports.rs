use std::net::{Ipv4Addr, TcpListener};

use super::*;
use crate::app::machine_links::acting::Aim;
use crate::app::menus::MenuAction;
use crate::app::more_menu::MoreItem;
use crate::app::overlay::Overlay;
use crate::app::tunnels::Origin;
use crate::control::ports::{Listing, PortInfo, PortStopped};
use crate::control::{HoldEnd, HoldEnded, PaneInfo, TabInfo, TunnelList};
use crate::ports::dialog::{Action, Scope};
use crate::ports::stop::Target;
use crate::test_util::{TempDir, fake_program};

const PANE: u64 = 31;
const TAB: u64 = 30;
const PID: i32 = 4242;

fn with_a_tab() -> control::Report {
    let mut report = holding();
    let pane = PaneInfo { id: PANE, ..PaneInfo::default() };
    let tab = TabInfo { id: TAB, name: "dev".into(), panes: vec![pane], ..TabInfo::default() };
    report.projects[0].workspaces[0].tabs = vec![tab];
    report
}

fn listening(port: u16) -> Listing {
    let info = PortInfo {
        port,
        addresses: vec!["127.0.0.1".into()],
        url: format!("http://localhost:{port}"),
        pid: PID,
        program: Some("python3".into()),
        args: vec!["-m".into(), "http.server".into(), format!("{port}")],
        project: Some(1),
        workspace: Some(SHOP),
        pane: Some(PANE),
        via: Some("pane".into()),
        agent: None,
        started: Some(77),
    };
    Listing { ports: vec![info] }
}

fn free_port() -> u16 {
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a free port");
    socket.local_addr().expect("its address").port()
}

impl Panels {
    fn with_tabs() -> Self {
        let mut panels = Self::linked();
        serve(&mut panels.link, 2, with_a_tab());
        let tabbed = |app: &App| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| !seen.projects[0].workspaces[0].tabs.is_empty())
        };
        panels.rig.until("the tab arrives", tabbed);
        panels
    }

    fn aim(&self) -> Aim {
        let link = self.rig.links()[0];
        self.rig.app.aim_at(link, SHOP).expect("the machine is held")
    }

    fn ports_open(&mut self, port: u16) -> UnixStream {
        let aim = self.aim();
        assert_eq!(self.rig.app.remote_action(MenuAction::RemotePorts(aim)), None);
        let mut stream = taken(&self.ends);
        let args = json!({ "workspace": SHOP, "all": false });
        assert_eq!(asked(&mut stream), json!({ "command": "ports", "boot": BOOT, "args": args }));
        tell(&mut stream, &listening(port));
        self.rig.until("the ports are listed", |app| matches!(&app.overlay, Some(Overlay::Ports(d)) if d.loaded));
        stream
    }

    fn ports(&self) -> Box<ui::Ports> {
        let Some(Overlay::Ports(dialog)) = &self.rig.app.overlay else { panic!("the ports dialog is not open") };
        self.rig.app.ports_view(dialog, Rect::new(0, 0, 120, 40))
    }
}

mod a_machine_s_ports {
    use super::*;

    #[test]
    fn are_listed_through_the_machine_with_its_tab_names() {
        let mut panels = Panels::with_tabs();

        panels.ports_open(8765);

        let view = panels.ports();
        assert_eq!(view.title, "Ports › vps › web-shop");
        assert_eq!(view.rows[0].url, "http://localhost:8765 on vps");
        assert_eq!(view.rows[0].place, "pane 31 · dev");
        assert!(view.rows[0].go);
        assert!(!view.rows[0].open);
    }

    #[test]
    fn are_stopped_by_the_machine_with_the_pid_that_was_listed() {
        let mut panels = Panels::with_tabs();
        panels.ports_open(8765);
        let target = Target { pid: PID, port: 8765, started: Some(77), program: Some("python3".into()) };

        panels.rig.app.ports_act(Action::Stop(vec![target]));
        let mut stream = taken(&panels.ends);
        let args = json!({ "port": 8765, "pid": PID, "started": 77 });
        assert_eq!(asked(&mut stream), json!({ "command": "stop-port", "boot": BOOT, "args": args }));
        let stopped = PortStopped { port: 8765, pid: PID, program: Some("python3".into()), outcome: "stopped".into() };
        tell(&mut stream, &stopped);

        let said = "stopped python3 (pid 4242) on port 8765 on vps";
        panels.rig.until("the stop is told", |app| app.messages.toast.as_ref().is_some_and(|t| t.message == said));
    }

    #[test]
    fn are_not_stopped_when_the_machine_did_not_say_when_they_started() {
        let mut panels = Panels::with_tabs();
        panels.ports_open(8765);
        let before = panels.rig.seen("start vps.lan");
        let target = Target { pid: PID, port: 8765, started: None, program: Some("python3".into()) };

        panels.rig.app.ports_act(Action::Stop(vec![target]));

        let said = panels.rig.app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
        assert!(
            said.starts_with("did not stop python3 (pid 4242) on port 8765 on vps: the machine did not say"),
            "{said}"
        );
        assert_eq!(panels.rig.seen("start vps.lan"), before);
    }

    #[test]
    fn are_not_opened_from_a_menu_of_an_earlier_run() {
        let mut panels = Panels::with_tabs();
        let mut aim = panels.aim();
        aim.era += 1;

        panels.rig.app.remote_action(MenuAction::RemotePorts(aim));

        assert!(panels.rig.app.overlay.is_none());
        assert_eq!(panels.rig.seen("start vps.lan"), 1);
    }

    #[test]
    fn go_shows_the_machine_s_tab() {
        let mut panels = Panels::with_tabs();
        panels.ports_open(8765);

        panels.rig.app.ports_act(Action::Go(0));

        assert!(panels.rig.app.overlay.is_none());
        assert_eq!(panels.rig.app.shown_remote().and_then(|shown| shown.tab), Some(TAB));
    }

    #[test]
    fn every_port_asks_the_machine_for_all_and_names_each_place() {
        let mut panels = Panels::with_tabs();
        panels.ports_open(8765);

        panels.rig.app.ports_act(Action::Scope(Scope::Every));
        let mut stream = taken(&panels.ends);
        assert_eq!(
            asked(&mut stream),
            json!({ "command": "ports", "boot": BOOT, "args": { "workspace": null, "all": true } })
        );
        let mut listing = listening(8765);
        let outside = PortInfo { port: 5432, workspace: None, pane: None, via: None, ..listing.ports[0].clone() };
        listing.ports.push(outside);
        tell(&mut stream, &listing);
        panels.rig.until("every port is listed", |app| matches!(&app.overlay, Some(Overlay::Ports(d)) if d.loaded));

        let view = panels.ports();
        assert_eq!(view.title, "Ports › vps");
        let places: Vec<(&str, bool)> = view.rows.iter().map(|row| (row.place.as_str(), row.tick.is_some())).collect();
        assert_eq!(places, [("web › web-shop · pane 31 · dev", true), ("no workspace", false)]);
    }

    #[test]
    fn more_opens_them_for_the_machine_s_workspace_on_screen() {
        let mut panels = Panels::with_tabs();
        panels.pick(SHOP);

        panels.rig.app.more_action(MoreItem::Ports, Position::default());

        let Some(Overlay::Ports(dialog)) = &panels.rig.app.overlay else { panic!("the ports dialog is not open") };
        assert_eq!(
            (dialog.there.as_ref().map(|there| there.machine.as_str()), dialog.workspace),
            (Some("vps"), Some(SHOP))
        );
    }

    #[test]
    fn are_not_asked_for_while_the_machine_is_down() {
        let mut panels = Panels::with_tabs();
        panels.ports_open(8765);

        panels.fall();
        panels.sync();

        let Some(ui::Note::Error(error)) = panels.ports().note else { panic!("no error is shown") };
        assert!(error.starts_with("could not list the ports: vps"), "{error}");
    }
}

mod a_tunnel_to_a_machine {
    use super::*;

    fn faked(panels: &mut Panels, dir: &TempDir) -> u16 {
        let ssh = fake_program(dir.path(), "ssh");
        std::fs::write(ssh.with_extension("forward"), "").expect("make it listen");
        std::fs::write(ssh.with_extension("hold"), "").expect("hold it");
        panels.rig.app.machines.tunnels.set_ssh(ssh.into_os_string());
        free_port()
    }

    fn tunnelled(panels: &mut Panels, dir: &TempDir) -> (u16, UnixStream) {
        let port = faked(panels, dir);
        let link = panels.rig.links()[0];
        panels.rig.app.open_there_tunnel(link, port, Origin::Click).expect("asked");
        let up = |app: &App| app.there_tunnel_infos().iter().any(|info| info.state == "up");
        panels.rig.until("the tunnel is up", up);
        let mut stream = taken(&panels.ends);
        let args = json!({ "port": port, "local": port });
        assert_eq!(asked(&mut stream), json!({ "command": "tunnel-hold", "boot": BOOT, "args": args }));
        tell(&mut stream, &TunnelList::default());
        (port, stream)
    }

    fn state(app: &App) -> Vec<(u16, Option<u16>, String)> {
        app.there_tunnel_infos().into_iter().map(|info| (info.port, info.local, info.state)).collect()
    }

    #[test]
    fn listens_here_and_is_held_on_the_machine() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());

        let (port, _stream) = tunnelled(&mut panels, &dir);

        assert_eq!(state(&panels.rig.app), [(port, Some(port), "up".to_string())]);
        held(&mut panels);
        let said = format!("port {port} of vps is tunnelled to localhost:{port}");
        assert_eq!(panels.rig.app.messages.toast.as_ref().map(|toast| toast.message.clone()), Some(said));
    }

    fn registered(app: &App) -> Option<bool> {
        app.there_tunnel_infos().first().and_then(|info| info.registered)
    }

    fn held(panels: &mut Panels) {
        panels.rig.until("the machine holds it", |app| registered(app) == Some(true));
    }

    #[test]
    fn is_no_longer_registered_once_its_ssh_ends() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());
        let (_port, _stream) = tunnelled(&mut panels, &dir);
        held(&mut panels);

        std::fs::remove_file(dir.path().join("ssh").with_extension("hold")).expect("let ssh end");

        panels.rig.until("the tunnel falls", |app| app.there_tunnel_infos().iter().any(|info| info.state == "down"));
        assert_eq!(registered(&panels.rig.app), None);
    }

    #[test]
    fn is_registered_again_with_a_machine_that_restarted() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());
        let (port, _stream) = tunnelled(&mut panels, &dir);
        held(&mut panels);

        serve(&mut panels.link, 3, control::Report { boot: "vps-2".into(), ..with_a_tab() });
        panels.rig.until("the restart is seen", |app| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.boot == "vps-2")
        });
        panels.sync();

        assert_eq!(registered(&panels.rig.app), None);
        let mut stream = taken(&panels.ends);
        let args = json!({ "port": port, "local": port });
        assert_eq!(asked(&mut stream), json!({ "command": "tunnel-hold", "boot": "vps-2", "args": args }));
    }

    #[test]
    fn marks_the_ports_row_and_opens_its_local_port() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());
        let (port, _stream) = tunnelled(&mut panels, &dir);

        panels.ports_open(port);

        let view = panels.ports();
        assert_eq!(view.rows[0].tunnel, Some(ui::PortTunnel::Up(port)));
    }

    #[test]
    fn ends_when_the_machine_closes_it() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());
        let (port, mut stream) = tunnelled(&mut panels, &dir);

        tell(&mut stream, &HoldEnded { ended: HoldEnd::Closed });

        panels.rig.until("the tunnel ends", |app| app.there_tunnel_infos().is_empty());
        let said = format!("vps closed the tunnel of its port {port}");
        assert_eq!(panels.rig.app.messages.toast.as_ref().map(|toast| toast.message.clone()), Some(said));
    }

    #[test]
    fn closing_it_here_ends_its_hold_on_the_machine() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());
        let (port, mut stream) = tunnelled(&mut panels, &dir);

        panels.rig.app.close_there_tunnel_of("vps", port);

        assert_eq!(panels.rig.app.there_tunnel_infos(), []);
        assert!(ended(&mut stream));
    }

    #[test]
    fn ends_when_the_machine_is_removed() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());
        let (_port, mut stream) = tunnelled(&mut panels, &dir);

        removed(&mut panels);
        panels.sync();

        assert_eq!(panels.rig.app.there_tunnel_infos(), []);
        assert!(ended(&mut stream));
    }

    #[test]
    fn is_refused_for_a_window_on_another_computer() {
        let mut panels = Panels::with_tabs();
        let shown = protocol::Machines { here: Some("laptop".into()), known: Vec::new() };
        panels.rig.app.set_machines(1, Some(shown));
        let link = panels.rig.links()[0];

        let refused = panels.rig.app.open_there_tunnel(link, 5173, Origin::Click);

        assert!(refused.is_err_and(|why| why.contains("another computer")));
        assert_eq!(panels.rig.app.there_tunnel_infos(), []);
    }

    fn ask(panels: &mut Panels, command: &str, args: &Value) {
        let text = json!({ "contract": 1, "command": command, "args": args }).to_string();
        panels.rig.app.request(9, &text, Some(Rect::new(0, 0, 120, 40)), Instant::now());
    }

    fn answered(panels: &mut Panels) -> Value {
        let mut said = Vec::new();
        wait_until("the request is answered", || {
            drain(&mut panels.rig.app, &panels.rig.rx);
            panels.sync();
            said.extend(panels.rig.app.take_answers());
            !said.is_empty()
        });
        serde_json::from_str(&said.remove(0).1).expect("a JSON answer")
    }

    #[test]
    fn is_opened_by_a_program_with_the_machine_s_name() {
        let (mut panels, dir) = (Panels::with_tabs(), TempDir::new());
        let port = faked(&mut panels, &dir);

        ask(&mut panels, "tunnel-open", &json!({ "port": port, "machine": "VPS" }));

        let answer = answered(&mut panels);
        let tunnel = &answer["ok"]["machine_tunnels"][0];
        assert_eq!((tunnel["machine"].as_str(), tunnel["state"].as_str()), (Some("vps"), Some("up")));
        assert_eq!(tunnel["local"].as_u64(), Some(u64::from(port)));
    }

    #[test]
    fn is_not_closed_in_reverse() {
        let mut panels = Panels::with_tabs();

        ask(&mut panels, "tunnel-close", &json!({ "port": 5173, "machine": "vps", "direction": "reverse" }));

        assert_eq!(answered(&mut panels)["error"]["code"], "malformed");
    }

    #[test]
    fn is_refused_for_a_machine_that_is_not_listed() {
        let mut panels = Panels::with_tabs();

        ask(&mut panels, "tunnel-open", &json!({ "port": 5173, "machine": "nas" }));

        let answer = answered(&mut panels);
        assert_eq!(answer["error"]["code"], "not-found");
        assert!(answer["error"]["message"].as_str().is_some_and(|said| said.contains("no machine is named nas")));
    }
}
