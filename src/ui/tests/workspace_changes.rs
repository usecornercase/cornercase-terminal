use super::*;

fn column(name: &str, behind: u32, changed: Option<Changed>) -> View<'static> {
    let entry =
        WorkspaceEntry { name: name.into(), behind, changed, tabs: vec!["zsh".into()], ..WorkspaceEntry::default() };
    View { has_project: true, workspaces: vec![entry], active_tab: Some(0), ..view(&["cornercase"]) }
}

fn changed(added: usize, removed: usize) -> Changed {
    Changed { added, removed }
}

fn drawn(v: &View) -> (String, Rect) {
    let rows = workspace_row(
        areas().workspaces_list,
        areas().pitch,
        &v.tab_lines(),
        v.workspaces_scroll,
        WorkspaceRow::Workspace(0),
    );
    (row_text(&render(v), rows), rows)
}

#[test]
fn the_count_follows_the_commits_to_pull() {
    let (text, _) = drawn(&column("login", 3, Some(changed(6, 9))));
    assert_eq!(text.trim_end(), "  ▾ login  ↓3 +6 \u{2212}9");
}

#[test]
fn added_is_green_and_removed_is_red() {
    let v = column("login", 0, Some(changed(6, 9)));
    let (text, row) = drawn(&v);
    let t = render(&v);
    let buf = t.backend().buffer();
    let at = |symbol: char| {
        let i = text.chars().position(|c| c == symbol).expect("symbol");
        buf[(row.x + u16::try_from(i).expect("cell"), row.y)].fg
    };
    assert_eq!((at('+'), at('\u{2212}')), (Color::Green, Color::Red));
}

#[test]
fn a_change_of_only_binary_files_says_zero_both_ways() {
    let (text, _) = drawn(&column("login", 0, Some(changed(0, 0))));
    assert_eq!(text.trim_end(), "  ▾ login     +0 \u{2212}0");
}

#[test]
fn a_clean_workspace_has_no_count() {
    let (text, _) = drawn(&column("login", 0, None));
    assert_eq!(text.trim_end(), "  ▾ login");
}

#[test]
fn it_gives_way_when_the_name_would_be_cut_short() {
    let (text, _) = drawn(&column("feature-with-a-very-long-name", 3, Some(changed(640, 9))));
    assert!(!text.contains('+'), "{text}");
    assert!(text.contains("↓3"), "{text}");
}

#[test]
fn it_is_dropped_before_the_commits_to_pull() {
    let (text, _) = drawn(&column("login", 3, Some(changed(123_456, 654_321))));
    assert!(!text.contains('+'), "{text}");
    assert!(text.contains("↓3"), "{text}");
}

#[test]
fn a_workspace_being_removed_has_none() {
    let mut v = column("login", 0, Some(changed(6, 9)));
    v.workspaces[0].removing = true;
    let (text, _) = drawn(&v);
    assert!(!text.contains('+'), "{text}");
}

#[test]
fn the_pull_badge_stays_where_it_is_drawn() {
    let mut v = column("login", 3, Some(changed(640, 9)));
    v.workspaces[0].pull = Some(crate::pull_request::Mark {
        number: 482,
        state: crate::pull_request::State::Open,
        ci: Some(crate::pull_request::Ci::Failing),
        conflicts: false,
        stack: None,
    });
    let target = workspace_tag(
        &workspace_layout(areas().workspaces_list, areas().pitch, &v.tab_lines(), v.workspaces_scroll),
        areas().pitch,
        &v.tab_lines(),
        0,
        (&v.workspaces[0], Tag::Pull),
    );
    let (text, row) = drawn(&v);
    let target = target.expect("the pull tag is drawn");
    let start = usize::from(target.x - row.x);
    let seen: String = text.chars().skip(start).take(usize::from(target.width)).collect();
    assert_eq!(seen, "#482✗", "{text}");
}
