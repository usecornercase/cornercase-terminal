use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::flows::builtin;
use crate::flows::run::{Kind, Message};
use crate::pull_request::{Ci, Mark, State as Pull};
use crate::ui::theme::{WAITING_COLOR, line_colour};

const MUTED: Color = Color::DarkGray;
const BRAND: Color = Color::Indexed(99);

fn flows() -> Vec<Flow> {
    builtin::flows(&[Known::Claude, Known::Codex, Known::Gemini, Known::Opencode], Some(Known::Claude))
}

fn full_issue() -> Flow {
    let mut flow = flows().remove(0);
    flow.roles[1].agent = "gemini".into();
    flow.roles[1].model = Some("gemini-2.5-pro".into());
    flow.roles[2].agent = "opencode".into();
    flow
}

fn render(d: &Diagram, width: u16) -> Terminal<TestBackend> {
    let height = diagram_height(d, width);
    let mut t = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    t.draw(|f| {
        let area = f.area();
        draw_diagram(f.buffer_mut(), area, d, 0, None);
    })
    .expect("draw");
    t
}

fn send(at: Instant, to: &str) -> Message {
    Message {
        from: ORCHESTRATOR.into(),
        to: to.into(),
        kind: Kind::Send,
        text: Some(format!("go {to}")),
        bytes: 0,
        at,
        time: std::time::SystemTime::UNIX_EPOCH,
        seq: 0,
        edge: None,
    }
}

fn progress(flow: &Flow, sends: &[&str], start: Instant) -> Progress {
    let messages: Vec<Message> =
        sends.iter().enumerate().map(|(i, to)| send(start + Duration::from_secs(i as u64), to)).collect();
    Progress::of(flow, &messages)
}

fn working(role: &str) -> State {
    match role {
        "reviewer" | "security" => State::Agent(Some(Status::Working)),
        "coder" => State::Agent(Some(Status::Done)),
        "tester" => State::Closed,
        _ => State::Agent(Some(Status::Idle)),
    }
}

fn cell(t: &Terminal<TestBackend>, x: u16, y: u16) -> &ratatui::buffer::Cell {
    &t.backend().buffer()[(x, y)]
}

fn find(t: &Terminal<TestBackend>, text: &str) -> Option<(u16, u16)> {
    let buf = t.backend().buffer();
    let first = text.chars().next()?;
    (0..buf.area.height).find_map(|y| {
        (0..buf.area.width)
            .find(|&x| {
                buf[(x, y)].symbol().starts_with(first)
                    && text.chars().enumerate().all(|(i, c)| {
                        u16::try_from(i)
                            .ok()
                            .and_then(|i| x.checked_add(i))
                            .filter(|&x| x < buf.area.width)
                            .is_some_and(|x| buf[(x, y)].symbol().starts_with(c))
                    })
            })
            .map(|x| (x, y))
    })
}

mod plan {
    use super::*;

    #[rstest]
    #[case::full_issue(0)]
    #[case::cross_review(1)]
    #[case::quick(2)]
    #[case::bug_test_first(3)]
    #[case::arena(4)]
    fn built_in_flows_at_46_columns(#[case] i: usize) {
        let flow = if i == 0 { full_issue() } else { flows().remove(i) };
        let d = Diagram::plan(&flow, MUTED, false);
        insta::assert_snapshot!(format!("plan_{i}_46"), render(&d, 46).backend());
    }

    #[rstest]
    #[case::narrow(30)]
    #[case::side_panel(40)]
    #[case::wide(60)]
    #[case::very_wide(100)]
    fn full_issue_at_other_widths(#[case] width: u16) {
        let d = Diagram::plan(&full_issue(), MUTED, false);
        insta::assert_snapshot!(format!("full_issue_{width}"), render(&d, width).backend());
    }

    #[test]
    fn arena_at_100_columns() {
        let d = Diagram::plan(&flows().remove(4), MUTED, false);
        insta::assert_snapshot!("arena_100", render(&d, 100).backend());
    }

    #[test]
    fn a_role_with_a_worktree_of_its_own_says_so_on_its_top_border() {
        let d = Diagram::plan(&flows().remove(4), MUTED, false);
        let t = render(&d, 100);
        let (x, y) = find(&t, " own ╮").expect("the short tag");
        assert_eq!(cell(&t, x + 1, y).fg, MUTED);
        assert_eq!(find(&t, "coder A").map(|(_, row)| row), Some(y));
        assert!(find(&t, "judge").is_some_and(|(_, row)| find(&t, " own ╮").is_some_and(|(_, own)| own != row)));
    }

    #[test]
    fn a_box_with_room_names_the_tag_in_full() {
        let mut flow = flows().remove(2);
        flow.roles[0].name = "a".into();
        flow.stages[0].roles[0] = "a".into();
        flow.roles[0].worktree = crate::flows::Worktree::Own;
        let d = Diagram::plan(&flow, MUTED, false);
        let t = render(&d, 60);
        assert!(find(&t, "own worktree").is_some(), "{:?}", t.backend());
    }

    #[test]
    fn a_wide_panel_keeps_the_diagram_at_its_own_width_in_the_middle() {
        let d = Diagram::plan(&full_issue(), MUTED, false);
        let (wide, natural) = (render(&d, 120), render(&d, d.natural_width()));
        let offset = (120 - d.natural_width()) / 2;
        let (x, y) = find(&natural, "orchestrator").expect("the orchestrator");
        assert_eq!(find(&wide, "orchestrator"), Some((x + offset, y)));
    }

    #[test]
    fn three_parallel_roles_stack_when_they_do_not_fit_side_by_side() {
        let mut flow = full_issue();
        flow.stages[1].roles.push("tester".into());
        flow.stages.remove(2);
        let d = Diagram::plan(&flow, MUTED, false);
        insta::assert_snapshot!("stacked_three", render(&d, 40).backend());
    }

    #[test]
    fn a_flow_being_edited_still_draws() {
        let mut flow = full_issue();
        flow.stages.push(crate::flows::Stage::default());
        flow.stages[0].roles.push("nobody".into());
        flow.stages[1].back = Some(crate::flows::Back { to: 7, ..crate::flows::Back::default() });
        let d = Diagram::plan(&flow, MUTED, false);
        insta::assert_snapshot!("being_edited", render(&d, 46).backend());
    }

    #[test]
    fn a_flow_without_stages_is_the_orchestrator_and_the_end() {
        let mut flow = full_issue();
        flow.stages.clear();
        let d = Diagram::plan(&flow, MUTED, false);
        assert_eq!(diagram_height(&d, 46), 5);
    }

    #[test]
    fn every_role_takes_its_own_colour_and_none_is_red_or_the_waiting_orange() {
        let colours: Vec<Color> = (0..6).map(role_colour).collect();
        let unique: std::collections::HashSet<_> = colours.iter().collect();
        let taken = [Color::Indexed(9), WAITING_COLOR].iter().any(|c| colours.contains(c));
        assert_eq!((unique.len(), taken), (6, false));
        assert_eq!(role_colour(6), role_colour(0));
    }

    #[test]
    fn names_are_coloured_and_lines_are_grey() {
        let d = Diagram::plan(&full_issue(), MUTED, false);
        let t = render(&d, 46);
        let (x, y) = find(&t, "coder").expect("the coder");
        assert_eq!(cell(&t, x, y).fg, role_colour(1));
        assert_eq!(cell(&t, x - 3, y).fg, line_colour(false));
        let (x, y) = find(&t, "▼").expect("an arrow");
        assert_eq!(cell(&t, x, y).fg, line_colour(false));
    }

    #[test]
    fn light_themes_take_the_light_line_colour() {
        let d = Diagram::plan(&full_issue(), MUTED, true);
        let t = render(&d, 46);
        let (x, y) = find(&t, "▼").expect("an arrow");
        assert_eq!(cell(&t, x, y).fg, line_colour(true));
    }
}

mod live {
    use super::*;

    fn diagram(sends: &[&str], now: Duration) -> (Diagram, Instant) {
        let flow = full_issue();
        let start = Instant::now();
        let progress = progress(&flow, sends, start);
        let at = start + Duration::from_secs(sends.len().saturating_sub(1) as u64) + now;
        (Diagram::plan(&flow, MUTED, false).live(&progress, working, Ending::default(), at), start)
    }

    #[test]
    fn working_roles_get_a_heavy_border_in_their_colour() {
        let (d, _) = diagram(&["coder", "reviewer", "security"], Duration::from_secs(5));
        let t = render(&d, 46);
        insta::assert_snapshot!("working", t.backend());
        let (x, y) = find(&t, "reviewer").expect("the reviewer");
        assert_eq!((cell(&t, x - 2, y).symbol(), cell(&t, x - 2, y).fg), ("━", role_colour(2)));
    }

    #[test]
    fn the_arrow_the_work_last_moved_along_takes_the_colour_of_where_it_came_from() {
        let (d, _) = diagram(&["coder", "reviewer"], Duration::from_secs(5));
        let t = render(&d, 46);
        let (x, y) = find(&t, "▼").expect("the arrow into the reviewers");
        assert_eq!(cell(&t, x, y).fg, role_colour(1));
    }

    #[test]
    fn a_dot_travels_along_the_arrow_for_a_second() {
        let (moving, _) = diagram(&["coder", "reviewer"], Duration::from_millis(500));
        let (still, _) = diagram(&["coder", "reviewer"], Duration::from_millis(1500));
        let dot = |d: &Diagram| d.live.as_ref().and_then(|l| l.lit.as_ref()).and_then(|lit| lit.dot);
        assert_eq!((dot(&moving), dot(&still)), (Some(0.5), None));
        assert!(find(&render(&moving, 46), "●").is_some());
        assert_eq!(find(&render(&still, 46), "●"), None);
    }

    #[test]
    fn a_loop_counts_its_rounds() {
        let (d, _) = diagram(&["coder", "reviewer", "coder", "reviewer", "coder"], Duration::from_secs(5));
        let t = render(&d, 46);
        insta::assert_snapshot!("looping", t.backend());
        assert!(find(&t, "findings · 2/3").is_some());
    }

    #[test]
    fn a_step_outside_the_plan_is_dashed() {
        let (d, _) = diagram(&["coder", "tester"], Duration::from_secs(5));
        let t = render(&d, 46);
        insta::assert_snapshot!("outside", t.backend());
        assert!(find(&t, "┆").is_some());
    }

    #[test]
    fn the_end_lights_from_what_it_is_given_not_from_the_orchestrator() {
        let flow = full_issue();
        let state = |status| move |_: &str| State::Agent(Some(status));
        let end = |reached| Ending { reached, pull: None };
        let colour = |state: &dyn Fn(&str) -> State, reached| {
            let d = Diagram::plan(&flow, MUTED, false).live(&Progress::default(), state, end(reached), Instant::now());
            let t = render(&d, 46);
            let (x, y) = find(&t, "◆").expect("the end");
            cell(&t, x, y).fg
        };
        assert_eq!(colour(&state(Status::Pending), false), MUTED);
        assert_eq!(colour(&state(Status::Working), true), BRAND);
    }
}

mod hits {
    use super::*;

    fn panel(diagram: Diagram) -> View {
        View { name: "Full issue".into(), task: "fix the login\nmore".into(), diagram, files: None, scroll: 0 }
    }

    const AREA: Rect = Rect::new(10, 2, 48, 60);

    fn position(view: &View, text: &str) -> Position {
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, view, None)).expect("draw");
        let (x, y) = find(&t, text).expect("on screen");
        Position::new(x, y)
    }

    fn cell_at(view: &View, at: Position) -> String {
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, view, None)).expect("draw");
        t.backend().buffer()[(at.x, at.y)].symbol().to_string()
    }

    #[test]
    fn a_box_names_its_role() {
        let view = panel(Diagram::plan(&full_issue(), MUTED, false));
        assert_eq!(hit(AREA, &view, position(&view, "security")), Some(Hit::Role("security".into())));
        assert_eq!(hit(AREA, &view, position(&view, "orchestrator")), Some(Hit::Role(ORCHESTRATOR.into())));
    }

    #[test]
    fn an_arrow_with_a_message_along_it_is_clickable() {
        let flow = full_issue();
        let start = Instant::now();
        let progress = progress(&flow, &["coder", "reviewer", "coder"], start);
        let d = Diagram::plan(&flow, MUTED, false).live(
            &progress,
            working,
            Ending::default(),
            start + Duration::from_secs(9),
        );
        let view = panel(d);
        let exit = position(&view, "┃─╯");
        assert_eq!(hit(AREA, &view, Position { x: exit.x + 1, ..exit }), Some(Hit::Edge(Edge::Loop(1), None)));
        assert_eq!(hit(AREA, &view, position(&view, "◀")), None);
        assert_eq!(hit(AREA, &view, position(&view, "findings")), Some(Hit::Edge(Edge::Loop(1), None)));
        assert_eq!(hit(AREA, &view, position(&view, "┴")), Some(Hit::Edge(Edge::Forward(0), None)));
    }

    #[test]
    fn a_branch_names_the_role_it_leads_to_and_takes_only_its_own_message() {
        let flow = full_issue();
        let start = Instant::now();
        let progress = progress(&flow, &["coder", "reviewer"], start);
        let d = Diagram::plan(&flow, MUTED, false).live(
            &progress,
            working,
            Ending::default(),
            start + Duration::from_secs(9),
        );
        let view = panel(d);
        let reviewer = position(&view, "▼");
        let security = Position { x: reviewer.x + 22, ..reviewer };
        assert_eq!(cell_at(&view, security), "▼");
        let forward = |to: &str| Some(Hit::Edge(Edge::Forward(0), Some(to.into())));
        assert_eq!(hit(AREA, &view, reviewer), forward("reviewer"));
        assert_eq!(hit(AREA, &view, Position { x: reviewer.x + 1, y: reviewer.y - 1 }), forward("reviewer"));
        assert_eq!(hit(AREA, &view, security), None);
    }

    #[test]
    fn hovering_a_branch_lights_its_way_and_not_the_others() {
        let flow = full_issue();
        let start = Instant::now();
        let progress = progress(&flow, &["coder", "reviewer", "security"], start);
        let d = Diagram::plan(&flow, MUTED, false).live(
            &progress,
            working,
            Ending::default(),
            start + Duration::from_secs(9),
        );
        let view = panel(d);
        let reviewer = position(&view, "▼");
        let security = Position { x: reviewer.x + 22, ..reviewer };
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, &view, Some(reviewer))).expect("draw");
        let fg = |at: Position| t.backend().buffer()[(at.x, at.y)].fg;
        let above = |at: Position| Position { y: at.y - 1, ..at };
        let trunk = position(&view, "┴");
        assert_eq!([fg(reviewer), fg(above(reviewer)), fg(trunk)], [Color::Cyan; 3]);
        assert_eq!([fg(security), fg(above(security))], [role_colour(1); 2]);
    }

    #[test]
    fn an_arrow_nothing_went_along_is_not() {
        let view = panel(Diagram::plan(&full_issue(), MUTED, false));
        assert_eq!(hit(AREA, &view, position(&view, "▼")), None);
    }

    #[test]
    fn the_close_button() {
        let view = panel(Diagram::plan(&full_issue(), MUTED, false));
        assert_eq!(hit(AREA, &view, close(AREA).as_position()), Some(Hit::Close));
    }

    #[test]
    fn scrolling_stops_at_the_bottom() {
        let view = panel(Diagram::plan(&full_issue(), MUTED, false));
        let short = Rect { height: 12, ..AREA };
        let most = diagram_height(&view.diagram, body(short).width) - body(short).height;
        assert_eq!((scrolled(short, &view, 100), scrolled(short, &view, -3)), (most, 0));
    }

    fn with_files(names: &[&str]) -> View {
        let rows = names.iter().map(|name| FileRow { name: (*name).into(), age: "2h".into() }).collect();
        View { files: Some(Files { rows, more: false }), ..panel(Diagram::plan(&full_issue(), MUTED, false)) }
    }

    #[test]
    fn the_files_follow_the_diagram_newest_first_with_their_ages() {
        let view = with_files(&["reviewer-2.md", "coder-1.md", "a-very-long-name-for-an-answer-file-that-is-cut.md"]);
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, &view, None)).expect("draw");
        insta::assert_snapshot!("panel_with_files", t.backend());
    }

    #[test]
    fn a_file_row_is_a_hit() {
        let view = with_files(&["reviewer-2.md", "coder-1.md"]);
        assert_eq!(hit(AREA, &view, position(&view, "coder-1.md")), Some(Hit::File(1)));
        assert_eq!(hit(AREA, &view, position(&view, "reviewer-2.md")), Some(Hit::File(0)));
        assert_eq!(hit(AREA, &view, position(&view, "Files")), None);
    }

    #[test]
    fn scrolling_reaches_the_last_file() {
        let view = with_files(&["a.md", "b.md", "last.md"]);
        let short = Rect { height: 12, ..AREA };
        let total = diagram_height(&view.diagram, body(short).width) + 5;
        let most = total - body(short).height;
        let at = scrolled(short, &view, 100);
        assert_eq!(at, most);
        let bottom = View { scroll: at, ..view };
        let mut t = Terminal::new(TestBackend::new(short.right(), short.bottom())).expect("terminal");
        t.draw(|f| draw(f, short, &bottom, None)).expect("draw");
        assert!(find(&t, "last.md").is_some());
    }

    #[test]
    fn an_empty_folder_says_nothing_yet() {
        let view = View { files: Some(Files { rows: Vec::new(), more: false }), ..with_files(&[]) };
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, &view, None)).expect("draw");
        assert!(find(&t, "nothing yet").is_some());
        assert_eq!(hit(AREA, &view, position(&view, "nothing yet")), None);
    }

    #[test]
    fn the_panel_shows_the_flow_and_the_first_line_of_the_task() {
        let view = panel(Diagram::plan(&full_issue(), MUTED, false));
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, &view, None)).expect("draw");
        assert_eq!((find(&t, "Full issue"), find(&t, "fix the login")), (Some((11, 2)), Some((11, 3))));
        assert_eq!(find(&t, "more"), None);
    }
}

mod limits {
    use super::*;

    fn renamed(mut flow: Flow, from: &str, to: &str) -> Flow {
        for role in flow.roles.iter_mut().filter(|role| role.name == from) {
            role.name = to.into();
        }
        for name in flow.stages.iter_mut().flat_map(|stage| &mut stage.roles).filter(|name| *name == from) {
            *name = to.into();
        }
        flow
    }

    pub(super) fn shown(t: &Terminal<TestBackend>, y: u16) -> String {
        let buf = t.backend().buffer();
        let mut row = String::new();
        let mut x = 0;
        while x < buf.area.width {
            let symbol = buf[(x, y)].symbol();
            row.push_str(symbol);
            x += u16::try_from(text::cells(symbol)).unwrap_or(1).max(1);
        }
        row
    }

    #[test]
    fn a_loop_label_longer_than_any_screen_takes_its_short_form() {
        let mut flow = full_issue();
        flow.stages[1].back.as_mut().expect("a loop").when = "w".repeat(40_000);
        let t = render(&Diagram::plan(&flow, MUTED, false), 80);
        assert!(find(&t, "≤3").is_some());
    }

    #[test]
    fn a_wide_name_is_cut_by_its_cells_and_stays_in_its_box() {
        let flow = renamed(renamed(full_issue(), "reviewer", "编码实现角色审查员"), "security", "安全审查角色");
        let d = Diagram::plan(&flow, MUTED, false);
        let t = render(&d, 46);
        let (x, y) = find(&t, "编").expect("the wide name");
        let row = shown(&t, y);
        assert_eq!(text::cells(&row), 46, "{row}");
        assert!(row.contains(" 编码实现角色… ") && row.contains(" 安全审查角色 "), "{row}");
        let (end, _) = find(&t, "…").expect("the cut");
        let area = Rect::new(0, 0, 46, diagram_height(&d, 46));
        let owners: Vec<_> = (x..=end).map(|x| diagram_hit(area, &d, 0, Position::new(x, y))).collect();
        assert!(owners.iter().all(|hit| *hit == Some(Hit::Role("编码实现角色审查员".into()))), "{owners:?}");
    }
}

mod end {
    use super::*;

    fn ended(pull: Option<Mark>, reached: bool) -> Diagram {
        Diagram::plan(&full_issue(), MUTED, false).live(
            &Progress::default(),
            |_| State::Agent(Some(Status::Idle)),
            Ending { reached, pull },
            Instant::now(),
        )
    }

    fn mark(state: Pull, ci: Option<Ci>, conflicts: bool) -> Mark {
        Mark { number: 485, state, ci, conflicts }
    }

    fn last_rows(t: &Terminal<TestBackend>, n: u16) -> String {
        let height = t.backend().buffer().area.height;
        (height - n..height).map(|y| limits::shown(t, y).trim_end().to_string()).collect::<Vec<_>>().join("\n")
    }

    #[rstest]
    #[case::failing(mark(Pull::Open, Some(Ci::Failing), false), "#485✗", Color::Red, Modifier::empty())]
    #[case::pending(mark(Pull::Open, Some(Ci::Pending), false), "#485•", Color::Yellow, Modifier::empty())]
    #[case::passing(mark(Pull::Open, Some(Ci::Passing), false), "#485✓", Color::Green, Modifier::empty())]
    #[case::conflict(mark(Pull::Open, Some(Ci::Passing), true), "#485!", WAITING_COLOR, Modifier::empty())]
    #[case::merged(mark(Pull::Merged, None, false), "#485", BRAND, Modifier::empty())]
    #[case::closed(mark(Pull::Closed, None, false), "#485", MUTED, Modifier::CROSSED_OUT)]
    #[case::no_checks(mark(Pull::Open, None, false), "#485", MUTED, Modifier::empty())]
    fn the_end_shows_the_pull_requests_tag_as_the_row_does(
        #[case] mark: Mark,
        #[case] text: &str,
        #[case] fg: Color,
        #[case] modifier: Modifier,
    ) {
        let t = render(&ended(Some(mark), true), 46);
        let (x, y) = find(&t, "#485").expect("the tag");
        assert_eq!(cell(&t, x, y).fg, fg);
        assert!(cell(&t, x, y).modifier.contains(modifier));
        assert!(limits::shown(&t, y).trim_end().ends_with(&format!("open a pull request · {text}")));
        assert_eq!((cell(&t, x - 2, y).symbol(), cell(&t, x - 2, y).fg), ("·", MUTED));
    }

    #[rstest]
    #[case::failing(mark(Pull::Open, Some(Ci::Failing), false))]
    #[case::pending(mark(Pull::Open, Some(Ci::Pending), false))]
    #[case::passing(mark(Pull::Open, Some(Ci::Passing), false))]
    #[case::conflict(mark(Pull::Open, Some(Ci::Passing), true))]
    #[case::merged(mark(Pull::Merged, None, false))]
    #[case::closed(mark(Pull::Closed, None, false))]
    #[case::none(mark(Pull::Open, None, false))]
    fn the_end_rows_hold_the_label_and_the_tag(#[case] mark: Mark) {
        let t = render(&ended(Some(mark), true), 46);
        let rows = last_rows(&t, 2);
        assert!(rows.contains("open a pull request") && rows.contains("#485"), "{rows}");
    }

    #[test]
    fn a_narrow_panel_cuts_the_label_first_and_keeps_the_tag() {
        let t = render(&ended(Some(mark(Pull::Open, Some(Ci::Passing), false)), true), 24);
        let rows = last_rows(&t, 2);
        assert!(rows.contains('…') && rows.trim_end().ends_with("· #485✓"), "{rows}");
        assert!(rows.lines().all(|row| text::cells(row) <= 24), "{rows}");
    }

    #[test]
    fn a_panel_too_narrow_for_the_label_keeps_only_the_tag() {
        let t = render(&ended(Some(mark(Pull::Open, Some(Ci::Passing), false)), true), 9);
        let rows = last_rows(&t, 2);
        assert!(rows.contains("#485✓") && !rows.contains('·'), "{rows}");
    }

    #[test]
    fn a_panel_too_narrow_for_the_tag_drops_it() {
        let t = render(&ended(Some(mark(Pull::Open, Some(Ci::Passing), false)), true), 4);
        assert!(find(&t, "#").is_none());
    }

    #[test]
    fn a_wide_diagram_has_room_beside_the_end() {
        let flow = flows().remove(4);
        let d = Diagram::plan(&flow, MUTED, false).live(
            &Progress::default(),
            |_| State::Agent(None),
            Ending { reached: true, pull: Some(mark(Pull::Open, Some(Ci::Passing), false)) },
            Instant::now(),
        );
        let t = render(&d, 100);
        let (_, y) = find(&t, "◆").expect("the end");
        let row = limits::shown(&t, y);
        assert!(row.contains("open a pull request · #485✓"), "{row}");
    }

    #[test]
    fn the_tag_does_not_widen_the_diagram() {
        let with = ended(Some(mark(Pull::Open, Some(Ci::Passing), false)), true);
        assert_eq!(with.natural_width(), ended(None, true).natural_width());
    }

    #[test]
    fn the_tag_is_a_hit_and_hovering_it_underlines_it() {
        let d = ended(Some(mark(Pull::Open, Some(Ci::Passing), false)), true);
        let t = render(&d, 46);
        let (x, y) = find(&t, "#485").expect("the tag");
        let area = Rect::new(0, 0, 46, diagram_height(&d, 46));
        for dx in 0..5 {
            assert_eq!(diagram_hit(area, &d, 0, Position::new(x + dx, y)), Some(Hit::Pull));
        }
        let (lx, ly) = find(&t, "open a").expect("the label");
        assert_eq!(diagram_hit(area, &d, 0, Position::new(lx, ly)), None);
        let mut buf = Terminal::new(TestBackend::new(46, area.height)).expect("terminal");
        buf.draw(|f| draw_diagram(f.buffer_mut(), area, &d, 0, Some(Position::new(x, y)))).expect("draw");
        assert!(cell(&buf, x, y).modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn a_plan_has_no_tag() {
        let t = render(&Diagram::plan(&full_issue(), MUTED, false), 46);
        assert!(find(&t, "#").is_none());
    }
}
