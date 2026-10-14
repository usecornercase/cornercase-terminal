use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::flows::builtin;
use crate::flows::run::{Kind, Message};
use crate::ui::theme::line_colour;

const MUTED: Color = Color::DarkGray;

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
    Message { from: ORCHESTRATOR.into(), to: to.into(), kind: Kind::Send, text: Some(format!("go {to}")), bytes: 0, at }
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
    fn every_role_takes_its_own_colour_and_none_is_red() {
        let colours: Vec<Color> = (0..7).map(role_colour).collect();
        let unique: std::collections::HashSet<_> = colours.iter().collect();
        assert_eq!((unique.len(), colours.contains(&Color::Indexed(9))), (7, false));
        assert_eq!(role_colour(7), role_colour(0));
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
        (Diagram::plan(&flow, MUTED, false).live(&progress, working, at), start)
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
        assert_eq!((moving.travelling(), still.travelling()), (true, false));
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
    fn the_end_lights_once_the_orchestrator_marks_the_run_for_review() {
        let flow = full_issue();
        let state = |role: &str| {
            if role == ORCHESTRATOR { State::Agent(Some(Status::Pending)) } else { State::Agent(Some(Status::Idle)) }
        };
        let d = Diagram::plan(&flow, MUTED, false).live(&Progress::default(), state, Instant::now());
        let t = render(&d, 46);
        let (x, y) = find(&t, "◆").expect("the end");
        assert_eq!(cell(&t, x, y).fg, BRAND);
    }

    const BRAND: Color = Color::Indexed(99);
}

mod hits {
    use super::*;

    fn panel(diagram: Diagram) -> View {
        View { name: "Full issue".into(), task: "fix the login\nmore".into(), diagram, scroll: 0 }
    }

    const AREA: Rect = Rect::new(10, 2, 48, 40);

    fn position(view: &View, text: &str) -> Position {
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, view, None)).expect("draw");
        let (x, y) = find(&t, text).expect("on screen");
        Position::new(x, y)
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
        let d = Diagram::plan(&flow, MUTED, false).live(&progress, working, start + Duration::from_secs(9));
        let view = panel(d);
        let exit = position(&view, "┃─╯");
        assert_eq!(hit(AREA, &view, Position { x: exit.x + 1, ..exit }), Some(Hit::Edge(Edge::Loop(1))));
        assert_eq!(hit(AREA, &view, position(&view, "◀")), None);
        assert_eq!(hit(AREA, &view, position(&view, "findings")), Some(Hit::Edge(Edge::Loop(1))));
        assert_eq!(hit(AREA, &view, position(&view, "▼")), Some(Hit::Edge(Edge::Forward(0))));
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

    #[test]
    fn the_panel_shows_the_flow_and_the_first_line_of_the_task() {
        let view = panel(Diagram::plan(&full_issue(), MUTED, false));
        let mut t = Terminal::new(TestBackend::new(AREA.right(), AREA.bottom())).expect("terminal");
        t.draw(|f| draw(f, AREA, &view, None)).expect("draw");
        assert_eq!((find(&t, "Full issue"), find(&t, "fix the login")), (Some((11, 2)), Some((11, 3))));
        assert_eq!(find(&t, "more"), None);
    }
}
