use ratatui::style::Color;

use super::*;
use crate::ports::dialog::Scope;

const EMPTY: &str = "Nothing in this workspace is listening on a port.";

const BIG: (u16, u16) = (120, 30);

const SMALL: (u16, u16) = (60, 28);

fn row(url: &str, command: &str, pid: i32, place: &str) -> PortRow {
    PortRow {
        tick: Some(false),
        url: url.into(),
        command: command.into(),
        pid,
        place: place.into(),
        go: true,
        open: true,
        tunnel: None,
    }
}

fn ports() -> Ports {
    Ports {
        title: "Ports › feat/checkout".into(),
        scope: Scope::Workspace,
        listening: Some(4),
        rows: vec![
            row("http://localhost:3000", "node next dev", 48211, "pane 12 · dev"),
            PortRow {
                tick: Some(true),
                ..row("http://localhost:5173", "node vite", 48390, "agent claude · pane 9 · claude")
            },
            PortRow {
                go: false,
                ..row("http://localhost:6006", "node storybook dev -p 6006", 51007, "in the worktree, no tab")
            },
            row("http://[::1]:9229", "node --inspect server.js", 51200, "pane 14 · api"),
        ],
        selected: Some(0),
        scroll: 0,
        ticked: 1,
        note: None,
        empty: None,
        confirm: None,
        compact: false,
    }
}

fn confirm() -> ConfirmBody {
    let warn = Style::default().fg(Color::Yellow);
    ConfirmBody {
        lines: vec![
            Line::from("Stop these 2 programs?"),
            Line::from("• node vite · pid 48390 · :5173 · agent claude · pane 9 · claude"),
            Line::from(Span::styled("  ⚠ it is the agent claude, or a program it started", warn)),
            Line::from("• node storybook dev -p 6006 · pid 51007 · :6006 · in the worktree, no tab"),
            Line::from(Span::styled("  ⚠ no tab of cornercase started it: it runs outside cornercase", warn)),
            Line::from(""),
            Line::from("Each gets SIGTERM, then SIGKILL if it still holds its port after 3 s."),
        ],
        scroll: 0,
    }
}

fn shown(p: Ports) -> View<'static> {
    View { overlay: Some(Overlay::Ports(Box::new(p))), ..view(&["web-shop"]) }
}

fn snapshot(p: Ports, (width, height): (u16, u16)) -> String {
    format!("{}", render_sized(&shown(p), width, height).backend())
}

fn screen((width, height): (u16, u16)) -> Rect {
    Rect::new(0, 0, width, height)
}

fn at(p: &Ports, size: (u16, u16), i: usize) -> Rect {
    let (_, list) = ports_room(screen(size), p.compact);
    let pitch = if p.compact { 3 } else { 1 };
    Rect { y: list.y + u16::try_from(i).expect("small") * pitch, height: pitch, ..list }
}

#[test]
fn ports_list() {
    insta::assert_snapshot!(snapshot(ports(), BIG));
}

#[test]
fn ports_list_looking() {
    let p = Ports {
        rows: Vec::new(),
        ticked: 0,
        selected: None,
        listening: None,
        note: Some(Note::Busy("looking for ports…")),
        ..ports()
    };
    insta::assert_snapshot!(snapshot(p, BIG));
}

#[test]
fn ports_empty() {
    let p = Ports { rows: Vec::new(), ticked: 0, selected: None, listening: Some(0), empty: Some(EMPTY), ..ports() };
    insta::assert_snapshot!(snapshot(p, BIG));
}

#[test]
fn ports_narrow() {
    insta::assert_snapshot!(snapshot(ports(), (90, 14)));
}

#[test]
fn ports_compact() {
    insta::assert_snapshot!(snapshot(Ports { compact: true, ..ports() }, SMALL));
}

#[test]
fn ports_confirm() {
    insta::assert_snapshot!(snapshot(Ports { confirm: Some(confirm()), ..ports() }, BIG));
}

#[test]
fn the_row_buttons_hit_their_action() {
    let p = ports();
    let r = at(&p, BIG, 0);
    let hit = |x: u16| ports_hit(screen(BIG), &p, Position::new(x, r.y));
    let right = r.right();
    assert_eq!(hit(right - 2), Some(PortsHit::StopRow(0)));
    assert_eq!(hit(right - 8), Some(PortsHit::Go(0)));
    assert_eq!(hit(right - 13), Some(PortsHit::Open(0)));
    assert_eq!(hit(right - 19), Some(PortsHit::Copy(0)));
    assert_eq!(hit(r.x + 1), Some(PortsHit::Tick(0)));
    assert_eq!(hit(r.x + 10), Some(PortsHit::Row(0)));
}

#[test]
fn a_window_that_cannot_open_has_no_open_button_and_a_row_without_a_pane_no_go() {
    let p = Ports { rows: ports().rows.into_iter().map(|row| PortRow { open: false, ..row }).collect(), ..ports() };
    let r = at(&p, BIG, 2);
    let hit = |x: u16| ports_hit(screen(BIG), &p, Position::new(x, r.y));
    assert_eq!(hit(r.right() - 2), Some(PortsHit::StopRow(2)));
    assert_eq!(hit(r.right() - 8), Some(PortsHit::Copy(2)));
}

#[test]
fn stop_is_off_without_ports_and_back_answers_in_the_confirmation() {
    let p = Ports { rows: Vec::new(), ticked: 0, selected: None, empty: Some(EMPTY), ..ports() };
    let area = settings_area(screen(BIG));
    let stop = Position::new(area.right() - 14, area.bottom() - 2);
    assert_ne!(ports_hit(screen(BIG), &p, stop), Some(PortsHit::Stop));
    let confirming = Ports { confirm: Some(confirm()), ..ports() };
    let back = Position::new(area.right() - 13, area.bottom() - 2);
    assert_eq!(ports_hit(screen(BIG), &confirming, back), Some(PortsHit::Back));
}

#[test]
fn ports_confirm_compact_overflowing() {
    let warn = Style::default().fg(Color::Yellow);
    let mut lines = vec![Line::from("Stop these 4 programs?")];
    for (command, port) in [("node next dev", 3000), ("node vite", 5173), ("node server.js", 9229)] {
        lines.push(Line::from(format!("• {command} · pid 48211 · :{port} ·")));
        lines.push(Line::from("pane 12 · dev"));
        lines.push(Line::from(Span::styled("  ⚠ it is the agent claude, or a", warn)));
        lines.push(Line::from(Span::styled("program it started", warn)));
    }
    let confirm = ConfirmBody { lines, scroll: 0 };
    let note = Some(Note::Info("↓ scroll for more".into()));
    let p = Ports { compact: true, confirm: Some(confirm), note, ..ports() };
    insta::assert_snapshot!(snapshot(p, (60, 12)));
}

fn tunnelled() -> Ports {
    let states = [Some(PortTunnel::Offered), Some(PortTunnel::Opening), Some(PortTunnel::Up(5174)), None];
    let opens = |tunnel: &Option<PortTunnel>| matches!(tunnel, Some(PortTunnel::Up(_)) | None);
    let rows = ports().rows.into_iter().zip(states);
    let rows = rows.map(|(row, tunnel)| PortRow { open: opens(&tunnel), tunnel, ..row }).collect();
    Ports { rows, ..ports() }
}

#[test]
fn ports_tunnels() {
    insta::assert_snapshot!(snapshot(tunnelled(), BIG));
}

#[test]
fn ports_tunnels_compact() {
    insta::assert_snapshot!(snapshot(Ports { compact: true, ..tunnelled() }, SMALL));
}

#[test]
fn a_tunnelled_row_offers_tunnel_or_close_tunnel() {
    let p = tunnelled();
    let hits = |row: usize| {
        let r = at(&p, BIG, row);
        let mut hits: Vec<PortsHit> =
            (r.x..r.right()).filter_map(|x| ports_hit(screen(BIG), &p, Position::new(x, r.y))).collect();
        hits.dedup();
        hits
    };
    assert!(hits(0).contains(&PortsHit::Tunnel(0)) && !hits(0).contains(&PortsHit::CloseTunnel(0)));
    assert!(hits(1).contains(&PortsHit::CloseTunnel(1)) && !hits(1).contains(&PortsHit::Open(1)));
    assert!(hits(2).contains(&PortsHit::CloseTunnel(2)) && hits(2).contains(&PortsHit::Open(2)));
    assert!(!hits(3).contains(&PortsHit::Tunnel(3)) && !hits(3).contains(&PortsHit::CloseTunnel(3)));
}

fn every() -> Ports {
    Ports {
        title: "Ports".into(),
        scope: Scope::Every,
        listening: Some(3),
        rows: vec![
            row("http://localhost:3000", "node next dev", 48211, "web-shop › feat/checkout · pane 12 · dev"),
            PortRow {
                go: false,
                tick: None,
                ..row("http://localhost:5432", "postgres -D /usr/local/var", 812, "no workspace")
            },
            row("http://localhost:8000", "python3 -m http.server", 51300, "blog › main · pane 3 · serve"),
        ],
        selected: Some(1),
        ticked: 0,
        note: Some(Note::Info("Only a workspace's programs can be stopped here.".into())),
        ..ports()
    }
}

#[test]
fn ports_every() {
    insta::assert_snapshot!(snapshot(every(), BIG));
}

mod every_port {
    use super::*;

    #[test]
    fn its_tabs_pick_the_scope() {
        let p = ports();
        let area = settings_area(screen(BIG));
        let (_, list) = ports_room(screen(BIG), false);
        let hit = |x: u16| ports_hit(screen(BIG), &p, Position::new(x, list.y - 2));
        assert_eq!(hit(area.x + 4), Some(PortsHit::Scope(Scope::Workspace)));
        assert_eq!(hit(area.x + 22), Some(PortsHit::Scope(Scope::Every)));
        assert_eq!(hit(area.x + 40), None);
    }

    #[test]
    fn a_row_of_no_workspace_has_no_tick_and_no_stop() {
        let p = every();
        let r = at(&p, BIG, 1);
        let hits: Vec<PortsHit> =
            (r.x..r.right()).filter_map(|x| ports_hit(screen(BIG), &p, Position::new(x, r.y))).collect();
        assert!(!hits.iter().any(|hit| matches!(hit, PortsHit::Tick(_) | PortsHit::StopRow(_))), "{hits:?}");
        assert!(hits.contains(&PortsHit::Copy(1)));
    }

    #[test]
    fn its_stop_button_waits_for_a_tick() {
        let area = settings_area(screen(BIG));
        let stop = Position::new(area.right() - 12, area.bottom() - 2);
        assert_eq!(ports_hit(screen(BIG), &every(), stop), None);
        let ticked = Ports { ticked: 1, ..every() };
        assert_eq!(ports_hit(screen(BIG), &ticked, stop), Some(PortsHit::Stop));
    }
}
