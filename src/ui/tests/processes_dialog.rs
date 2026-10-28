use ratatui::style::Color;

use super::*;

const BIG: (u16, u16) = (120, 30);

const SMALL: (u16, u16) = (60, 28);

fn pane(text: &str, program: &str, cpu: f64, memory: u64, open: bool) -> ProcessLine {
    ProcessLine { depth: None, open, text: text.into(), program: program.into(), cpu, memory, pane: true, stop: false }
}

fn process(depth: usize, command: &str, pid: i32, cpu: f64, memory: u64) -> ProcessLine {
    ProcessLine {
        depth: Some(depth),
        open: false,
        text: command.into(),
        program: format!("pid {pid}"),
        cpu,
        memory,
        pane: false,
        stop: depth > 0,
    }
}

fn processes() -> Processes {
    let cornercase = ProcessLine { pane: false, ..pane("cornercase", "server", 0.8, 85 << 20, false) };
    Processes {
        filter: String::new(),
        sort: 0,
        totals: "3 panes · 7 processes · cpu 136.0% · 1.9 GB".into(),
        lines: vec![
            pane("web-shop › fix/login › dev", "npm", 98.0, 812 << 20, true),
            process(0, "zsh", 4200, 0.0, 4 << 20),
            process(1, "npm run dev", 4210, 0.1, 60 << 20),
            process(2, "node vite --port 5173", 4211, 97.9, 740 << 20),
            pane("web-shop › main › claude", "claude", 36.5, 1200 << 20, false),
            pane("api › main › server", "cargo", 1.2, 300 << 20, false),
            cornercase,
        ],
        selected: Some(0),
        scroll: 0,
        empty: None,
        note: None,
        confirm: None,
        compact: false,
    }
}

fn confirm() -> ConfirmBody {
    let warn = Style::default().fg(Color::Yellow);
    ConfirmBody {
        lines: vec![
            Line::from("Stop this program and the programs it started?"),
            Line::from("• node vite --port 5173 · pid 4211 · web-shop › fix/login › dev"),
            Line::from("  also stops: esbuild (pid 4230), node (pid 4231)"),
            Line::from(Span::styled(
                "  ⚠ the agent claude started it: the agent may start it again, or fail without it",
                warn,
            )),
            Line::from(""),
            Line::from(
                "Each gets SIGTERM, then SIGKILL if it is still running after 3 s. The pane's shell keeps running.",
            ),
        ],
        scroll: 0,
    }
}

fn shown(p: Processes) -> View<'static> {
    View { overlay: Some(Overlay::Processes(Box::new(p))), ..view(&["web-shop"]) }
}

fn snapshot(p: Processes, (width, height): (u16, u16)) -> String {
    format!("{}", render_sized(&shown(p), width, height).backend())
}

fn screen((width, height): (u16, u16)) -> Rect {
    Rect::new(0, 0, width, height)
}

fn at(p: &Processes, size: (u16, u16), i: usize) -> Rect {
    let (_, list) = processes_room(screen(size), p.compact);
    let pitch = if p.compact { 3 } else { 1 };
    Rect { y: list.y + u16::try_from(i).expect("small") * pitch, height: pitch, ..list }
}

#[test]
fn processes_list() {
    insta::assert_snapshot!(snapshot(processes(), BIG));
}

#[test]
fn processes_sorted_by_memory() {
    insta::assert_snapshot!(snapshot(Processes { sort: 1, ..processes() }, BIG));
}

#[test]
fn processes_filtered() {
    let p = Processes {
        filter: "claude".into(),
        lines: vec![pane("web-shop › main › claude", "claude", 36.5, 1200 << 20, false)],
        ..processes()
    };
    insta::assert_snapshot!(snapshot(p, BIG));
}

#[test]
fn processes_no_matches() {
    let p =
        Processes { filter: "zzz".into(), lines: Vec::new(), selected: None, empty: Some("no matches"), ..processes() };
    insta::assert_snapshot!(snapshot(p, BIG));
}

#[test]
fn processes_measuring() {
    let p = Processes {
        lines: Vec::new(),
        selected: None,
        totals: "0 panes · 0 processes · cpu 0.0% · 0 MB".into(),
        note: Some(Note::Busy("measuring…")),
        ..processes()
    };
    insta::assert_snapshot!(snapshot(p, BIG));
}

#[test]
fn processes_empty() {
    let p = Processes { lines: Vec::new(), selected: None, empty: Some("No pane is open."), ..processes() };
    insta::assert_snapshot!(snapshot(p, BIG));
}

#[test]
fn processes_narrow() {
    insta::assert_snapshot!(snapshot(processes(), (90, 14)));
}

#[test]
fn processes_compact() {
    insta::assert_snapshot!(snapshot(Processes { compact: true, ..processes() }, SMALL));
}

#[test]
fn processes_confirm() {
    insta::assert_snapshot!(snapshot(Processes { confirm: Some(confirm()), ..processes() }, BIG));
}

#[test]
fn processes_confirm_compact_overflowing() {
    let mut lines = vec![Line::from("Stop this program and the programs it started?")];
    lines.extend((0..12).map(|i| Line::from(format!("• node worker {i} · pid {}", 4000 + i))));
    let note = Some(Note::Info("↓ scroll for more".into()));
    let p = Processes { compact: true, confirm: Some(ConfirmBody { lines, scroll: 0 }), note, ..processes() };
    insta::assert_snapshot!(snapshot(p, (60, 12)));
}

#[test]
fn a_hovered_or_selected_row_shows_its_buttons_and_a_pane_row_its_go_and_close() {
    let p = processes();
    let r = at(&p, BIG, 0);
    let hit = |x: u16, y: u16| processes_hit(screen(BIG), &p, Position::new(x, y));
    assert_eq!(hit(r.right() - 2, r.y), Some(ProcessesHit::Close(0)));
    assert_eq!(hit(r.right() - 9, r.y), Some(ProcessesHit::Go(0)));
    assert_eq!(hit(r.x + 10, r.y), Some(ProcessesHit::Row(0)));
    let line = at(&p, BIG, 2);
    assert_eq!(hit(line.right() - 2, line.y), Some(ProcessesHit::StopRow(2)));
    let shell = at(&p, BIG, 1);
    assert_eq!(hit(shell.right() - 2, shell.y), Some(ProcessesHit::Row(1)));
    let own = at(&p, BIG, 6);
    assert_eq!(hit(own.right() - 2, own.y), Some(ProcessesHit::Row(6)));
}

#[test]
fn the_sort_tabs_filter_and_done_answer_clicks_and_back_and_stop_answer_in_the_confirmation() {
    let p = processes();
    let area = settings_area(screen(BIG));
    let hit = |p: &Processes, pos: Position| processes_hit(screen(BIG), p, pos);
    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    assert_eq!(hit(&p, Position::new(inner.x + 4, inner.y)), Some(ProcessesHit::Filter));
    assert_eq!(hit(&p, Position::new(inner.right() - 2, inner.y)), Some(ProcessesHit::Sort(2)));
    assert_eq!(hit(&p, Position::new(inner.right() - 6 - 2 - 7, inner.y)), Some(ProcessesHit::Sort(1)));
    assert_eq!(hit(&p, Position::new(inner.right() - 6, inner.bottom() - 1)), Some(ProcessesHit::Done));
    let confirming = Processes { confirm: Some(confirm()), ..processes() };
    assert_eq!(hit(&confirming, Position::new(inner.right() - 3, inner.bottom() - 1)), Some(ProcessesHit::Stop));
    assert_eq!(hit(&confirming, Position::new(inner.right() - 11, inner.bottom() - 1)), Some(ProcessesHit::Back));
    assert_eq!(hit(&confirming, Position::new(inner.x + 4, inner.y)), None);
}

#[test]
fn the_border_is_drawn_at_the_areas_corners() {
    let p = processes();
    let overlay = Overlay::Processes(Box::new(p));
    let area = overlay.area(screen(BIG)).expect("an area");
    let terminal = render_sized(&shown(processes()), BIG.0, BIG.1);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(area.x, area.y)].symbol(), "╭");
    assert_eq!(buffer[(area.right() - 1, area.bottom() - 1)].symbol(), "╯");
}
