use super::*;
use crate::issues::browser::IssuesHit;

const TABS: [&str; 4] = ["All", "GitHub", "Shortcut", "Linear"];
const TOGGLES: [&str; 2] = ["closed", "mine"];

fn row(key: &str, title: &str, meta: &str) -> IssueRow {
    IssueRow { key: key.into(), title: title.into(), meta: meta.into(), colour: None }
}

fn issues(body: IssuesBody) -> Issues {
    Issues {
        title: "Issues · shop".into(),
        tabs: TABS.to_vec(),
        tab: 0,
        toggles: TOGGLES.map(String::from).to_vec(),
        on: vec![false, true],
        body,
        note: None,
        hint: "enter reads #482 · start works on it in its own worktree, on issue-482-returns".into(),
        buttons: vec!["start", "cancel"],
    }
}

fn list(selected: Option<usize>) -> IssuesBody {
    IssuesBody::List {
        filter: String::new(),
        items: vec![
            row("#482", "Returns page crashes on empty address", "bug · ana · 3d"),
            row("sc-48", "Dark mode", "In Review · luis · 2mo"),
        ],
        selected,
        scroll: 0,
        empty: "no open issues".into(),
    }
}

fn with(issues: Issues) -> View<'static> {
    View { overlay: Some(Overlay::Issues(issues)), ..view(&["shop"]) }
}

#[test]
fn are_drawn_where_their_area_says() {
    assert_eq!(corners_of_its_area(&with(issues(list(None)))), (String::from("╭"), String::from("╯")));
}

fn item(i: usize) -> Position {
    list_item(issues_list(issues_area(AREA)), 2, 0, i).as_position()
}

fn text(v: &View) -> String {
    render(v).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect()
}

#[test]
fn renders_the_tabs_toggles_and_issues() {
    insta::assert_snapshot!(render(&with(issues(list(Some(0))))).backend());
}

#[test]
fn renders_the_token_form() {
    let body = IssuesBody::Token {
        label: "API token",
        input: "•••".into(),
        help: vec!["Connect Shortcut.".into(), String::new(), "Paste a token.".into()],
    };
    let i = Issues { tab: 2, buttons: vec!["connect", "cancel"], ..issues(body) };
    insta::assert_snapshot!(render(&with(i)).backend());
}

fn naming() -> Issues {
    let body = IssuesBody::Name {
        heading: "Start #482 Returns page crashes in a new worktree".into(),
        fields: [
            ("branch   ", "issue-482-returns-page-crashes".into()),
            ("workspace", "#482 Returns page crashes".into()),
        ],
        focus: 1,
        help: vec!["A branch that exists is checked out.".into()],
    };
    Issues { buttons: vec!["start", "back"], hint: "enter starts".into(), ..issues(body) }
}

#[test]
fn renders_the_branch_and_the_workspace_s_name() {
    insta::assert_snapshot!(render(&with(naming())).backend());
}

#[rstest]
#[case::branch(issue_name_fields(issues_area(AREA))[0].as_position(), Some(IssuesHit::Field(0)))]
#[case::name(issue_name_fields(issues_area(AREA))[1].as_position(), Some(IssuesHit::Field(1)))]
#[case::start(issue_buttons(issues_area(AREA), &["start", "back"])[0].as_position(), Some(IssuesHit::Button(0)))]
#[case::nothing(Position::new(0, 0), None)]
fn maps_clicks_on_the_names(#[case] pos: Position, #[case] expected: Option<IssuesHit>) {
    assert_eq!(issue_name_hit(AREA, &["start", "back"], pos), expected);
}

#[test]
fn renders_an_issue_from_its_scroll() {
    let lines = (0..40).map(|n| Line::from(format!("line {n}"))).collect();
    let i = Issues { buttons: vec!["start", "back"], ..issues(IssuesBody::Detail { lines, scroll: 5 }) };
    let first = issue_detail(issues_area(AREA));
    let t = render(&with(i));
    let row: String =
        (first.x..first.right()).map(|x| t.backend().buffer()[(x, first.y)].symbol().to_string()).collect();
    assert!(row.starts_with("line 5"), "{row:?}");
}

#[test]
fn the_active_tab_is_filled() {
    let i = issues(list(None));
    let tab = issue_tabs(issues_area(AREA), &TABS)[0].as_position();
    assert_eq!(render(&with(i)).backend().buffer()[tab].bg, DARK_SURFACE);
}

#[test]
fn a_toggle_shows_whether_it_is_on() {
    let t = text(&with(issues(list(None))));
    assert!(t.contains("[ ] closed") && t.contains("[x] mine"), "{t}");
}

#[test]
fn highlights_the_selected_issue() {
    assert_eq!(render(&with(issues(list(Some(1))))).backend().buffer()[item(1)].bg, DARK_SURFACE);
}

#[test]
fn highlights_the_hovered_issue() {
    let v = View { hover: Some(item(1)), ..with(issues(list(None))) };
    assert_eq!(render(&v).backend().buffer()[item(1)].bg, DARK_HOVER);
}

#[test]
fn says_why_the_list_is_empty() {
    let body = IssuesBody::List {
        filter: "zzz".into(),
        items: Vec::new(),
        selected: None,
        scroll: 0,
        empty: "no matches".into(),
    };
    assert!(text(&with(issues(body))).contains("no matches"));
}

#[test]
fn an_error_replaces_the_hint() {
    let i = Issues { note: Some(Note::Error("no git remotes found".into())), ..issues(list(None)) };
    let t = text(&with(i));
    assert!(t.contains("no git remotes found") && !t.contains("enter reads"), "{t}");
}

#[rstest]
#[case::tab(issue_tabs(issues_area(AREA), &TABS)[3].as_position(), Some(IssuesHit::Tab(3)))]
#[case::toggle(issue_toggles(issues_area(AREA), &TOGGLES)[1].as_position(), Some(IssuesHit::Toggle(1)))]
#[case::issue(item(1), Some(IssuesHit::Item(1)))]
#[case::below_the_last(item(1).offset(ratatui::layout::Offset { x: 0, y: 1 }), None)]
#[case::start(issue_buttons(issues_area(AREA), &["start", "cancel"])[0].as_position(), Some(IssuesHit::Button(0)))]
#[case::cancel(issue_buttons(issues_area(AREA), &["start", "cancel"])[1].as_position(), Some(IssuesHit::Button(1)))]
fn maps_clicks(#[case] pos: Position, #[case] expected: Option<IssuesHit>) {
    assert_eq!(
        issues_hit(
            AREA,
            Labels { tabs: &TABS, toggles: &TOGGLES, buttons: &["start", "cancel"] },
            Shown { items: 2, scroll: 0 },
            pos
        ),
        expected
    );
}

#[test]
fn buttons_end_at_the_right_edge_in_order() {
    let b = issue_buttons(issues_area(AREA), &["start", "disconnect", "cancel"]);
    assert!(
        b[0].right() < b[1].x && b[1].right() < b[2].x && b[2].right() == issues_rows(issues_area(AREA))[4].right()
    );
}

#[test]
fn the_button_shows_only_with_a_project() {
    let shown = |v: &View| text(v).contains(ISSUES_LABEL);
    let project = View { has_project: true, issues: true, ..view(&["shop"]) };
    assert_eq!((shown(&view(&[])), shown(&project)), (false, true));
}

#[test]
fn the_button_sits_level_with_settings() {
    assert_eq!(areas().issues.y, areas().settings.y);
}

#[test]
fn the_button_is_filled_on_hover() {
    let pos = areas().issues.as_position().offset(ratatui::layout::Offset { x: 2, y: 0 });
    let v = View { has_project: true, issues: true, hover: Some(pos), ..view(&["shop"]) };
    assert_eq!(render(&v).backend().buffer()[pos].bg, Color::Cyan);
}
