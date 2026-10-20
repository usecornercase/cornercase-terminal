use super::*;

fn wlist() -> Rect {
    areas().workspaces_list
}

fn at(row: u16) -> Position {
    Position::new(wlist().x + 4, wlist().y + row)
}

fn close_at(row: u16) -> Position {
    Position::new(wlist().right() - 2, wlist().y + row)
}

fn with_workspaces(active_tab: Option<usize>) -> View<'static> {
    View {
        has_project: true,
        workspaces: vec![
            WorkspaceEntry {
                collapsed: false,
                name: "login".into(),
                tabs: vec!["claude".into(), "nvim".into()],
                behind: 0,
                pull: None,
                removing: false,
                setting_up: false,
            },
            WorkspaceEntry {
                collapsed: false,
                name: "main".into(),
                tabs: vec!["zsh".into()],
                behind: 0,
                pull: None,
                removing: false,
                setting_up: false,
            },
        ],
        active_tab,
        ..view(&["cornercase"])
    }
}

fn row_text(v: &View, row: WorkspaceRow) -> String {
    let r = workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), v.workspaces_scroll, row);
    let t = render(v);
    (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
}

#[rstest]
#[case::workspace(WorkspaceRow::Workspace(0))]
#[case::tab(WorkspaceRow::Tab(0, 0))]
fn long_names_keep_their_start(#[case] row: WorkspaceRow) {
    let long = "feature-with-a-very-long-name-that-does-not-fit";
    let mut v = with_workspaces(Some(0));
    v.workspaces[0].name = long.into();
    v.workspaces[0].tabs[0] = long.into();
    let text = row_text(&v, row);
    assert!(text.contains("feature-with") && text.contains('…'), "{text:?}");
}

#[test]
fn rows_are_each_workspace_then_its_tabs_then_plus_tab() {
    use WorkspaceRow::{Gap, NewTab, Tab, Workspace};
    assert_eq!(
        workspace_rows(&tabs(&[2, 0])),
        [Workspace(0), Tab(0, 0), Tab(0, 1), NewTab(0), Gap, Workspace(1), NewTab(1)]
    );
}

#[rstest]
#[case::workspace(at(0), Some(WorkspaceHit::Workspace(0)))]
#[case::workspace_close(close_at(0), Some(WorkspaceHit::CloseWorkspace(0)))]
#[case::tab(at(1), Some(WorkspaceHit::Tab(0, 0)))]
#[case::tab_close(close_at(2), Some(WorkspaceHit::CloseTab(0, 1)))]
#[case::plus_tab(at(3), Some(WorkspaceHit::NewTab(0)))]
#[case::gap_before_new_workspace(at(4), None)]
#[case::new_workspace(at(5), Some(WorkspaceHit::NewWorkspace))]
fn maps_clicks(#[case] pos: Position, #[case] expected: Option<WorkspaceHit>) {
    assert_eq!(workspace_hit(wlist(), 1, &tabs(&[2]), 0, pos), expected);
}

#[test]
fn the_gap_between_workspaces_is_not_a_row() {
    assert_eq!(workspace_hit(wlist(), 1, &tabs(&[0, 0]), 0, at(2)), None);
}

#[test]
fn new_workspace_sticks_to_the_bottom_when_rows_overflow() {
    assert_eq!(new_workspace_button(wlist(), 1, &tabs(&[100])).y, wlist().bottom() - 1);
}

#[test]
fn rows_under_the_new_workspace_button_are_hidden() {
    let last_visible = usize::from(wlist().height) - 1;
    assert!(workspace_row(wlist(), 1, &tabs(&[100]), 0, WorkspaceRow::Tab(0, last_visible)).is_empty());
}

#[test]
fn renders_the_workspaces_of_the_active_project() {
    insta::assert_snapshot!(render(&with_workspaces(Some(0))).backend());
}

#[test]
fn renders_commits_to_pull_before_the_close_button() {
    let mut v = View { hover: Some(at(0)), ..with_workspaces(Some(0)) };
    v.workspaces[0].behind = 3;
    v.workspaces[1].behind = 1;
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn renders_a_workspace_being_removed_without_its_buttons() {
    let mut v = View { hover: Some(at(0)), active_workspace: 1, ..with_workspaces(Some(0)) };
    v.workspaces[0] = WorkspaceEntry {
        collapsed: false,
        behind: 3,
        pull: None,
        removing: true,
        setting_up: false,
        tabs: Vec::new(),
        name: "login".into(),
    };
    insta::assert_snapshot!(render(&v).backend());
}

#[rstest]
#[case::nothing_to_pull("login", 0, "  ▾ login")]
#[case::some("login", 3, "  ▾ login        ↓3")]
#[case::long_name_is_cut_first("feature-with-a-very-long-name", 3, "  ▾ feature-wit… ↓3")]
fn commits_to_pull_sit_at_the_end_of_the_row(#[case] name: &str, #[case] behind: u32, #[case] expected: &str) {
    let mut v = with_workspaces(Some(0));
    v.workspaces[0] = WorkspaceEntry { collapsed: false, name: name.into(), behind, ..v.workspaces.remove(0) };
    assert_eq!(row_text(&v, WorkspaceRow::Workspace(0)).trim_end(), expected);
}

#[test]
fn marks_the_active_tab() {
    let pos = Position::new(wlist().x, wlist().y + 1);
    assert_eq!(render(&with_workspaces(Some(0))).backend().buffer()[pos].symbol(), "▌");
}

#[test]
fn plus_tab_is_filled_on_hover() {
    let v = View { hover: Some(at(3)), ..with_workspaces(Some(0)) };
    assert_eq!(render(&v).backend().buffer()[at(3)].bg, Color::Cyan);
}

#[test]
fn an_empty_workspace_says_so_in_the_pane() {
    let v = View {
        workspaces: vec![WorkspaceEntry {
            collapsed: false,
            name: "main".into(),
            tabs: Vec::new(),
            behind: 0,
            pull: None,
            removing: false,
            setting_up: false,
        }],
        ..with_workspaces(None)
    };
    let text: String = render(&v).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(text.contains("no tab open"), "{text}");
}

#[test]
fn nothing_shows_without_a_project() {
    let text: String =
        render(&view(&[])).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(!text.contains("New workspace"), "{text}");
}

mod pull_request_tag {
    use super::*;
    use crate::pull_request::{Ci, Mark, State};

    fn tagged(name: &str, behind: u32, mark: Mark) -> View<'static> {
        let mut v = with_workspaces(Some(0));
        let first = &mut v.workspaces[0];
        (first.name, first.behind, first.pull) = (name.into(), behind, Some(mark));
        v
    }

    fn mark(state: State, ci: Option<Ci>) -> Mark {
        Mark { number: 482, state, ci, conflicts: false }
    }

    fn conflicting(state: State, ci: Option<Ci>) -> Mark {
        Mark { conflicts: true, ..mark(state, ci) }
    }

    fn tag_style(v: &View) -> (Color, bool) {
        let text = row_text(v, WorkspaceRow::Workspace(0));
        let x = wlist().x + u16::try_from(text.find('#').expect("a tag")).expect("narrow");
        let terminal = render(v);
        let cell = &terminal.backend().buffer()[(x, wlist().y)];
        (cell.fg, cell.modifier.contains(Modifier::CROSSED_OUT))
    }

    #[rstest]
    #[case::failing(mark(State::Open, Some(Ci::Failing)), "#482✗", Color::Red, false)]
    #[case::running(mark(State::Open, Some(Ci::Pending)), "#482•", Color::Yellow, false)]
    #[case::passing(mark(State::Draft, Some(Ci::Passing)), "#482✓", Color::Green, false)]
    #[case::no_checks(mark(State::Open, None), "#482", Color::DarkGray, false)]
    #[case::merged(mark(State::Merged, Some(Ci::Failing)), "#482", Color::Indexed(99), false)]
    #[case::closed(mark(State::Closed, Some(Ci::Passing)), "#482", Color::DarkGray, true)]
    #[case::conflicting_with_passing_checks(
        conflicting(State::Open, Some(Ci::Passing)),
        "#482!",
        Color::Indexed(208),
        false
    )]
    #[case::conflicting_before_failing_checks(
        conflicting(State::Open, Some(Ci::Failing)),
        "#482!",
        Color::Indexed(208),
        false
    )]
    #[case::conflicting_draft_without_checks(conflicting(State::Draft, None), "#482!", Color::Indexed(208), false)]
    #[case::merged_never_conflicts(conflicting(State::Merged, None), "#482", Color::Indexed(99), false)]
    fn says_the_state_and_the_checks(
        #[case] mark: Mark,
        #[case] tag: &str,
        #[case] colour: Color,
        #[case] crossed: bool,
    ) {
        let v = tagged("login", 0, mark);

        assert_eq!(row_text(&v, WorkspaceRow::Workspace(0)).trim_end(), format!("  ▾ login{tag:>10}"));
        assert_eq!(tag_style(&v), (colour, crossed));
    }

    #[rstest]
    #[case::before_commits_to_pull("login", "  ▾ login  #482✓ ↓3")]
    #[case::a_long_name_is_cut_first("feature-with-a-very-long-name", "  ▾ featu… #482✓ ↓3")]
    fn sits_at_the_end_of_the_row(#[case] name: &str, #[case] expected: &str) {
        let v = tagged(name, 3, mark(State::Open, Some(Ci::Passing)));

        assert_eq!(row_text(&v, WorkspaceRow::Workspace(0)).trim_end(), expected);
    }

    fn badge_text(v: &View) -> Option<String> {
        let r = workspace_pull(
            &workspace_layout(wlist(), areas().pitch, &v.tab_lines(), v.workspaces_scroll),
            areas().pitch,
            &v.tab_lines(),
            0,
            &v.workspaces[0],
        )?;
        let terminal = render(v);
        let buffer = terminal.backend().buffer();
        Some((r.x..r.right()).map(|x| buffer[(x, r.y)].symbol().to_string()).collect())
    }

    #[rstest]
    #[case::alone("login", 0, false, "#482✗")]
    #[case::before_commits_to_pull("login", 3, false, "#482✗")]
    #[case::after_the_attention_icon("login", 3, true, "#482✗")]
    #[case::a_long_name_is_cut("feature-with-a-very-long-name", 3, false, "#482✗")]
    #[case::a_wide_name("日本語の名前", 3, false, "#482✗")]
    fn the_target_is_the_badge_drawn(
        #[case] name: &str,
        #[case] behind: u32,
        #[case] waiting: bool,
        #[case] expected: &str,
    ) {
        let mut v = tagged(name, behind, mark(State::Open, Some(Ci::Failing)));
        if waiting {
            v.workspaces[0].tabs[0].status = Some(Status::Waiting);
        }

        assert_eq!(badge_text(&v).as_deref(), Some(expected));
    }

    #[test]
    fn a_conflict_mark_is_the_target_too() {
        let v = tagged("login", 0, conflicting(State::Open, Some(Ci::Passing)));

        assert_eq!(badge_text(&v).as_deref(), Some("#482!"));
    }

    #[test]
    fn a_name_allocated_one_cell_does_not_shift_the_badge_from_its_target() {
        let mut v = tagged("login", 3, mark(State::Open, Some(Ci::Failing)));
        v.workspaces[0].tabs[0].status = Some(Status::Waiting);
        let list = Rect { width: 20, ..wlist() };
        let r = workspace_pull(
            &workspace_layout(list, areas().pitch, &v.tab_lines(), 0),
            areas().pitch,
            &v.tab_lines(),
            0,
            &v.workspaces[0],
        )
        .expect("a target");
        let row = workspace_row(list, areas().pitch, &v.tab_lines(), 0, WorkspaceRow::Workspace(0));
        let mut terminal = Terminal::new(TestBackend::new(160, 60)).expect("terminal");
        terminal
            .draw(|f| {
                let band = Band { r: row, pitch: 1, lead: vec![Span::raw("    ")], bg: Style::default() };
                let line = workspaces::WorkspaceLine {
                    entry: &v.workspaces[0],
                    active: false,
                    badge: workspaces::Badge::Attention,
                };
                workspaces::draw_workspace_band(f, &v, band, line);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();

        let drawn: String = (r.x..r.right()).map(|x| buffer[(x, r.y)].symbol().to_string()).collect();

        assert_eq!(drawn, "#482✗");
    }

    #[test]
    fn in_compact_mode_the_target_is_the_whole_band() {
        let v = tagged("login", 0, mark(State::Open, Some(Ci::Failing)));
        let pitch = 3;

        let r = workspace_pull(
            &workspace_layout(wlist(), pitch, &v.tab_lines(), 0),
            pitch,
            &v.tab_lines(),
            0,
            &v.workspaces[0],
        )
        .expect("a target");

        assert_eq!(r.height, pitch);
        assert_eq!(r.width, 5);
    }

    fn pull_badge_for(row: Rect, entry: &WorkspaceEntry) -> Option<Rect> {
        workspaces::pull_badge(row, 1, 4, entry, workspaces::Badge::Attention)
    }

    #[test]
    fn a_narrow_row_drops_the_badge_and_its_target() {
        let v = tagged("login", 0, mark(State::Open, Some(Ci::Failing)));
        let row = Rect::new(0, 0, 8, 1);

        assert!(pull_badge_for(row, &v.workspaces[0]).is_none());
        assert!(pull_badge_for(Rect::new(0, 0, 30, 1), &v.workspaces[0]).is_some());
    }

    #[test]
    fn a_row_being_removed_has_no_target() {
        let mut v = tagged("login", 0, mark(State::Open, Some(Ci::Failing)));
        v.workspaces[0].removing = true;

        assert_eq!(badge_text(&v), None);
    }

    fn badge_cell(v: &View) -> ratatui::buffer::Cell {
        let r = workspace_pull(
            &workspace_layout(wlist(), areas().pitch, &v.tab_lines(), 0),
            areas().pitch,
            &v.tab_lines(),
            0,
            &v.workspaces[0],
        )
        .expect("a target");
        render(v).backend().buffer()[(r.x, r.y)].clone()
    }

    #[test]
    fn hovering_it_underlines_it_and_keeps_its_colour() {
        let mut v = tagged("login", 0, mark(State::Open, Some(Ci::Failing)));
        let r = workspace_pull(
            &workspace_layout(wlist(), areas().pitch, &v.tab_lines(), 0),
            areas().pitch,
            &v.tab_lines(),
            0,
            &v.workspaces[0],
        )
        .expect("a target");
        v.hover = Some(r.as_position());

        let cell = badge_cell(&v);

        assert!(cell.modifier.contains(Modifier::UNDERLINED));
        assert_eq!(cell.fg, Color::Red);
    }

    #[test]
    fn hovering_the_name_does_not() {
        let mut v = tagged("login", 0, mark(State::Open, Some(Ci::Failing)));
        v.hover = Some(at(0));

        assert!(!badge_cell(&v).modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn no_underline_while_an_overlay_is_open() {
        let mut v = tagged("login", 0, mark(State::Open, Some(Ci::Failing)));
        let r = workspace_pull(
            &workspace_layout(wlist(), areas().pitch, &v.tab_lines(), 0),
            areas().pitch,
            &v.tab_lines(),
            0,
            &v.workspaces[0],
        )
        .expect("a target");
        v.hover = Some(r.as_position());
        v.overlay = Some(Overlay::Menu { at: Position::new(0, 0), items: vec!["rename".into()] });

        assert!(!badge_cell(&v).modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn renders_beside_the_other_marks() {
        let mut v = tagged("login", 3, mark(State::Open, Some(Ci::Failing)));
        v.workspaces[1].pull = Some(mark(State::Merged, None));
        insta::assert_snapshot!(render(&v).backend());
    }
}
