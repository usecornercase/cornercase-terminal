use super::*;
use crate::pull_request::Outcome;

fn new_workspace_form(on: bool) -> Form {
    Form {
        title: "New workspace",
        label: "name",
        value: "feat/login".into(),
        hint: "in ~/.cornercase/worktrees/cornercase/feat-login".into(),
        toggle: Some(Toggle { label: "with its own worktree", on }),
        note: None,
        submit: "create",
    }
}

fn with(overlay: Overlay) -> View<'static> {
    View { overlay: Some(overlay), ..view(&["cornercase"]) }
}

fn remote_access(url: Option<&str>) -> RemoteAccess {
    RemoteAccess {
        title: remote_access::TITLE,
        url: url.map(String::from),
        code: url.map(remote_access::code).unwrap_or_default(),
        lines: vec![
            "scan the code with your phone's camera.".into(),
            "the phone needs the Tailscale app, logged in as ada@example.com.".into(),
        ],
    }
}

const PAIR_URL: &str = "https://mac.tail1234.ts.net:8443/pair?t=0123456789abcdef0123456789abcdef";

#[test]
fn renders_the_remote_code() {
    let view = with(Overlay::Remote(remote_access(Some(PAIR_URL))));
    insta::assert_snapshot!(render_sized(&view, W, 40).backend());
}

#[test]
fn a_short_window_shows_the_remote_address_without_the_code() {
    insta::assert_snapshot!(render(&with(Overlay::Remote(remote_access(Some(PAIR_URL))))).backend());
}

#[test]
fn the_remote_dialog_is_drawn_where_its_area_says() {
    let corners = corners_of_its_area(&with(Overlay::Remote(remote_access(None))));
    assert_eq!(corners, (String::from("╭"), String::from("╯")));
}

#[test]
fn the_remote_button_lights_green_while_a_browser_is_attached() {
    let view = View { remote: Some(true), ..view(&["cornercase"]) };
    let t = render(&view);
    let r = remote_button(layout(AREA, Widths::default()).usage, true);
    let dot = t.backend().buffer()[(r.right() - 2, r.y)].clone();
    assert_eq!((dot.symbol(), dot.fg), ("●", Color::Green));
}

#[test]
fn hovering_the_remote_button_leaves_usage_unlit() {
    let usage = layout(AREA, Widths::default()).usage;
    let remote = remote_button(usage, false);
    let view = View { remote: Some(false), hover: Some(remote.as_position()), ..view(&["cornercase"]) };
    let t = render(&view);
    let cells = (t.backend().buffer()[(remote.x, remote.y)].bg, t.backend().buffer()[(usage.x, usage.y)].bg);
    assert_eq!(cells, (Color::Cyan, Color::Reset));
}

#[test]
fn a_form_and_the_usage_are_drawn_where_their_area_says() {
    let form = corners_of_its_area(&with(Overlay::Form(new_workspace_form(true))));
    let usage = corners_of_its_area(&with(Overlay::Usage(usage())));
    assert_eq!((form, usage), ((String::from("╭"), String::from("╯")), (String::from("╭"), String::from("╯"))));
}

#[test]
fn renders_the_worktree_toggle() {
    insta::assert_snapshot!(render(&with(Overlay::Form(new_workspace_form(true)))).backend());
}

#[test]
fn the_toggle_row_is_hit() {
    let pos = form_toggle(form_area(AREA)).as_position();
    assert_eq!(form_hit(AREA, "create", None, pos), Some(FormHit::Toggle));
}

fn usage_window(label: &str, percent: u16, severity: Severity, resets: &str) -> UsageWindow {
    UsageWindow { label: label.into(), percent, severity, resets: resets.into() }
}

fn claude_usage() -> UsageSection {
    UsageSection {
        title: "Claude Code · max plan".into(),
        status: "updated 2m ago".into(),
        error: None,
        windows: vec![
            usage_window("session (5h)", 7, Severity::Normal, "resets in 2h 13m"),
            usage_window("week", 82, Severity::Warning, "resets in 3d 4h"),
            usage_window("week · Fable", 100, Severity::Critical, ""),
        ],
        empty: None,
        extra: Some("extra usage: 12.34 USD of 50.00 USD".into()),
    }
}

fn codex_usage() -> UsageSection {
    UsageSection {
        title: "Codex · plus plan".into(),
        status: "updated just now".into(),
        error: None,
        windows: vec![
            usage_window("session (5h)", 53, Severity::Normal, "resets in 4h 59m"),
            usage_window("week", 8, Severity::Normal, "resets in 5d 22h"),
        ],
        empty: None,
        extra: None,
    }
}

fn usage() -> Usage {
    Usage {
        page: crate::agents::usage::Page::Limits,
        stats: crate::agents::stats::View::default(),
        sections: vec![claude_usage()],
    }
}

fn stats_usage(empty: bool) -> Usage {
    let now = std::time::UNIX_EPOCH + std::time::Duration::from_hours(10_000);
    let stats = if empty { crate::agents::stats::Stats::new(now) } else { crate::agents::stats::tests::example(now) };
    Usage { page: crate::agents::usage::Page::Stats, stats: stats.view(now), sections: Vec::new() }
}

fn merged_usage(complete: bool) -> Usage {
    let mut usage = stats_usage(false);
    usage.stats.merged =
        Some(crate::agents::stats::MergedRow { day: 2, week: 12, total: 61, complete, accounts: Vec::new() });
    usage
}

#[test]
fn renders_merged_pull_requests_in_the_stats() {
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(merged_usage(true))), W, 38).backend());
}
#[test]
fn renders_the_merged_row_before_any_agent_work_was_counted() {
    let mut usage = stats_usage(true);
    usage.stats.merged =
        Some(crate::agents::stats::MergedRow { day: 1, week: 2, total: 3, complete: true, accounts: Vec::new() });
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(usage)), W, 38).backend());
}
#[test]
fn renders_a_partial_merged_count_with_a_plus() {
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(merged_usage(false))), 46, 38).backend());
}
#[test]
fn renders_work_stats_in_the_wide_dialog() {
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(stats_usage(false))), W, 38).backend());
}
#[test]
fn renders_work_stats_at_phone_width() {
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(stats_usage(false))), 46, 38).backend());
}
#[test]
fn renders_stats_before_any_work_was_counted() {
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(stats_usage(true))), W, 38).backend());
}
#[rstest]
#[case::dark(false, [24, 26, 33, 75])]
#[case::light(true, [153, 75, 27, 19])]
fn the_heatmap_has_a_blue_ramp_for_each_theme(#[case] light: bool, #[case] palette: [u8; 4]) {
    let mut view = with(Overlay::Usage(stats_usage(false)));
    view.light = light;
    for (i, expected) in palette.iter().enumerate() {
        assert_eq!(stats::colour(&view, u8::try_from(i + 1).expect("level")), Color::Indexed(*expected));
    }
    assert_eq!(stats::colour(&view, 0), view.line());
    insta::assert_snapshot!(
        format!("stats_palette_{}", if light { "light" } else { "dark" }),
        render_sized(&view, W, 38).backend()
    );
}
#[test]
fn usage_tabs_and_border_share_the_drawn_geometry() {
    let usage = stats_usage(false);
    let area = usage_area(AREA, &usage);
    assert_eq!(usage_tabs(AREA, &usage), tabs_in(usage_rows(area)[0], &crate::agents::usage::Page::NAMES));
    let corners = corners_of_its_area(&with(Overlay::Usage(usage)));
    assert_eq!(corners, ("╭".into(), "╯".into()));
}

const TALL: u16 = 34;

#[test]
fn renders_the_usage_windows() {
    insta::assert_snapshot!(render(&with(Overlay::Usage(usage()))).backend());
}

#[test]
fn renders_one_section_per_agent() {
    let usage = Usage {
        page: crate::agents::usage::Page::Limits,
        stats: crate::agents::stats::View::default(),
        sections: vec![claude_usage(), codex_usage()],
    };
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(usage)), W, TALL).backend());
}

#[test]
fn renders_usage_while_loading_and_after_a_failure() {
    let loading = UsageSection { status: "loading…".into(), windows: Vec::new(), extra: None, ..claude_usage() };
    let failed = UsageSection {
        title: "Codex".into(),
        status: String::new(),
        error: Some(
            "usage unavailable: could not run /opt/codex/bin/codex: No such file or directory (os error 2)".into(),
        ),
        windows: Vec::new(),
        ..codex_usage()
    };
    let usage = Usage {
        page: crate::agents::usage::Page::Limits,
        stats: crate::agents::stats::View::default(),
        sections: vec![loading, failed],
    };
    insta::assert_snapshot!(render(&with(Overlay::Usage(usage))).backend());
}

#[rstest]
#[case::normal(0, Color::Green)]
#[case::warning(1, Color::Indexed(208))]
#[case::critical(2, Color::Red)]
fn usage_bars_take_the_severity_colour(#[case] window: u16, #[case] expected: Color) {
    let t = render(&with(Overlay::Usage(usage())));
    let body = usage_rows(usage_area(AREA, &usage()))[1];
    let bar = Position::new(body.x, body.y + 3 + window * 3);
    assert_eq!(t.backend().buffer()[bar].fg, expected);
}

#[test]
fn the_rest_of_a_usage_bar_takes_the_line_colour() {
    let t = render(&with(Overlay::Usage(usage())));
    let body = usage_rows(usage_area(AREA, &usage()))[1];
    let bar = Position::new(body.right() - 1, body.y + 3);
    assert_eq!(t.backend().buffer()[bar].fg, DARK_LINE);
}

#[test]
fn a_dialog_dims_what_is_behind_it() {
    let t = render(&with(Overlay::Form(new_workspace_form(true))));
    let form = form_area(AREA);
    let buffer = t.backend().buffer();
    let behind = buffer[areas().title.as_position()].modifier.contains(Modifier::DIM);
    let inside = buffer[form_rows(form)[1].as_position()].modifier.contains(Modifier::DIM);
    assert_eq!((behind, inside), (true, false));
}

#[test]
fn the_usage_done_button_sits_on_the_last_row() {
    let r = usage_area(AREA, &usage());
    assert_eq!(usage_done(AREA, &usage()).y, r.bottom() - 2);
}

#[test]
fn a_usage_too_tall_for_the_screen_keeps_its_done_button() {
    let usage = Usage {
        page: crate::agents::usage::Page::Limits,
        stats: crate::agents::stats::View::default(),
        sections: vec![claude_usage(), codex_usage()],
    };
    let r = usage_area(AREA, &usage);
    assert_eq!((r.height, usage_done(AREA, &usage).y), (AREA.height, r.bottom() - 2));
}

#[test]
fn renders_a_confirmation() {
    let confirm = Confirm {
        title: "Remove workspace",
        message: "Remove the workspace login and delete its worktree folder ~/wt/login? The branch is kept.".into(),
        note: Some(Note::Error("contains modified or untracked files, use --force to delete it".into())),
        submit: "remove anyway",
        extra: None,
    };
    insta::assert_snapshot!(render(&with(Overlay::Confirm(confirm))).backend());
}

#[test]
fn renders_a_confirmation_with_a_second_choice() {
    let confirm = Confirm {
        title: "Delete group",
        message: "Delete the group work? Keep its 2 projects open, ungrouped, or close them too, which stops \
                    3 tabs and the programs running in them. Folders and worktrees stay on disk."
            .into(),
        note: None,
        submit: "keep projects",
        extra: Some("close projects"),
    };
    insta::assert_snapshot!(render(&with(Overlay::Confirm(confirm))).backend());
}

#[test]
fn renders_a_second_button_left_of_the_first() {
    let notes = vec![
        Line::from("open · 1 failed, 1 passed"),
        Line::default(),
        Line::from(vec![outcome_icon(Color::DarkGray, Outcome::Failed), Span::raw(" tests")]),
        Line::from(vec![outcome_icon(Color::DarkGray, Outcome::Passed), Span::raw(" lint")]),
    ];
    let update = Update {
        title: "Pull request",
        message: "#482 Returns page crashes on an empty address".into(),
        notes,
        scroll: 0,
        note: None,
        submit: "copy link",
        extra: Some("ask agent"),
        cancel: "done",
    };
    let v = with(Overlay::Update(update));
    let extra = update_extra(AREA, "copy link", "done", "ask agent");
    let [submit, _] = update_buttons(AREA, "copy link", "done");
    assert_eq!(extra.right() + 1, submit.x);
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn renders_an_update_with_its_notes() {
    let notes = (1..=40).map(|i| Line::from(format!("• change {i}"))).collect();
    let update = Update {
        title: "Update",
        message: "cornercase 9.0.0 is out (you have 0.1.0). Updating replaces ~/.local/bin/cornercase; \
                    your terminals keep running until you restart."
            .into(),
        notes,
        scroll: 30,
        note: Some(Note::Busy("downloading…")),
        submit: "update",
        extra: None,
        cancel: CANCEL_LABEL,
    };
    insta::assert_snapshot!(render(&with(Overlay::Update(update))).backend());
}
