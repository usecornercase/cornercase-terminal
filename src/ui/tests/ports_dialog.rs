use ratatui::style::Color;

use super::*;

const BIG: (u16, u16) = (120, 30);

const SMALL: (u16, u16) = (60, 28);

fn row(url: &str, command: &str, pid: i32, place: &str) -> PortRow {
    PortRow { ticked: false, url: url.into(), command: command.into(), pid, place: place.into(), go: true }
}

fn ports() -> Ports {
    Ports {
        title: "Ports › feat/checkout".into(),
        rows: vec![
            row("http://localhost:3000", "node next dev", 48211, "pane 12 · dev"),
            PortRow {
                ticked: true,
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
        empty: false,
        confirm: None,
        can_open: true,
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
    let p =
        Ports {
            rows: Vec::new(), ticked: 0, selected: None, note: Some(Note::Busy("looking for ports…")), ..ports()
        };
    insta::assert_snapshot!(snapshot(p, BIG));
}

#[test]
fn ports_empty() {
    let p = Ports { rows: Vec::new(), ticked: 0, selected: None, empty: true, ..ports() };
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
    let p = Ports { can_open: false, ..ports() };
    let r = at(&p, BIG, 2);
    let hit = |x: u16| ports_hit(screen(BIG), &p, Position::new(x, r.y));
    assert_eq!(hit(r.right() - 2), Some(PortsHit::StopRow(2)));
    assert_eq!(hit(r.right() - 8), Some(PortsHit::Copy(2)));
}

#[test]
fn stop_is_off_without_ports_and_back_answers_in_the_confirmation() {
    let p = Ports { rows: Vec::new(), ticked: 0, selected: None, empty: true, ..ports() };
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
