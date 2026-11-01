use super::*;
use crate::editor::Editor;
use crate::pull_request::Outcome;

fn new_workspace_form(on: bool) -> Form {
    Form {
        title: "New workspace",
        label: "name",
        value: Editor::new("feat/login"),
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

#[rstest]
#[case::attached(Some(true), Some(("●", Color::Green)))]
#[case::set_up(Some(false), None)]
#[case::off(None, None)]
fn more_shows_a_green_dot_while_a_browser_is_attached(
    #[case] remote: Option<bool>,
    #[case] dot: Option<(&str, Color)>,
) {
    let t = render(&View { remote, ..view(&["cornercase"]) });
    let more = layout(AREA, Widths::default()).more;
    let label = more.x + 2 + u16::try_from(MORE_LABEL.chars().count()).expect("short");
    let cell = t.backend().buffer()[(label + 1, more.y)].clone();
    assert_eq!((cell.symbol() != " ").then(|| (cell.symbol(), cell.fg)), dot);
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
        on: None,
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
        on: None,
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
    Usage { page: crate::agents::usage::Page::Limits, sections: vec![claude_usage()], ..Usage::default() }
}

fn stats_usage(empty: bool) -> Usage {
    let now = std::time::UNIX_EPOCH + std::time::Duration::from_hours(10_000);
    let stats = if empty { crate::agents::stats::Stats::new(now) } else { crate::agents::stats::tests::example(now) };
    Usage { page: crate::agents::usage::Page::Stats, stats: stats.view(now), ..Usage::default() }
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
    assert_eq!(usage_tabs(AREA, &usage), tabs_in(usage_rows(area, &usage)[1], &crate::agents::usage::Page::NAMES));
    let corners = corners_of_its_area(&with(Overlay::Usage(usage)));
    assert_eq!(corners, ("╭".into(), "╯".into()));
}

fn picked_usage(picked: usize, notes: &[&str]) -> Usage {
    Usage {
        machines: ["this computer", "vps", "all"].map(String::from).to_vec(),
        picked,
        notes: notes.iter().map(ToString::to_string).collect(),
        ..stats_usage(false)
    }
}

#[test]
fn renders_the_machine_picker_on_every_machine() {
    let usage = picked_usage(2, &[crate::agents::stats::PEAK, "vps: seen 5 min ago"]);
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(usage)), W, 38).backend());
}

#[test]
fn renders_the_stats_of_one_machine() {
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(picked_usage(1, &[]))), W, 38).backend());
}

#[test]
fn renders_the_machine_picker_at_phone_width() {
    let usage = picked_usage(2, &[crate::agents::stats::PEAK, "pi: not summed · not read yet"]);
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(usage)), 46, 42).backend());
}

#[test]
fn the_machine_picker_sits_above_the_tabs_and_only_with_machines() {
    let usage = picked_usage(2, &[]);
    let rows = usage_rows(usage_area(AREA, &usage), &usage);
    assert_eq!(usage_picks(AREA, &usage), tabs_in(rows[0], &["this computer", "vps", "all"]));
    assert_eq!((rows[1].y, usage_tabs(AREA, &usage)[0].y), (rows[0].y + 2, rows[1].y));
    assert_eq!(usage_picks(AREA, &stats_usage(false)), Vec::<Rect>::new());
}

fn on(name: &str, status: &str, error: Option<&str>, sections: Vec<UsageSection>) -> UsageMachine {
    UsageMachine { name: name.into(), status: status.into(), error: error.map(str::to_string), sections }
}

fn merged_limits(title: &str) -> Usage {
    let merged = UsageSection { title: title.into(), on: Some("on this computer, vps".into()), ..claude_usage() };
    Usage {
        page: crate::agents::usage::Page::Limits,
        by_machine: vec![
            on("this computer", "", None, vec![merged]),
            on("vps", "seen 5 min ago", None, vec![codex_usage()]),
            on("pi", "", Some("not seen yet"), Vec::new()),
        ],
        machines: ["this computer", "vps", "pi", "all"].map(String::from).to_vec(),
        picked: 3,
        ..Usage::default()
    }
}

#[test]
fn renders_the_limits_of_every_machine_with_a_merged_row() {
    let usage = merged_limits("Claude Code · max plan");
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(usage)), W, 44).backend());
}

#[test]
fn renders_a_merged_row_with_a_label_at_phone_width_without_cutting_its_status() {
    let usage = merged_limits("Claude Code · work ● · max plan");
    insta::assert_snapshot!(render_sized(&with(Overlay::Usage(usage)), 46, 48).backend());
}

#[test]
fn every_machine_in_the_picker_stays_clickable_at_phone_width() {
    let names = ["this computer", "vps", "mini", "pi", "laptop", "desk", "nas", "build", "studio", "all"];
    let usage = Usage { machines: names.map(String::from).to_vec(), ..picked_usage(0, &[]) };
    let phone = Rect::new(0, 0, 46, 42);
    let row = usage_rows(usage_area(phone, &usage), &usage)[0];

    let picks = usage_picks(phone, &usage);

    assert!(picks.iter().all(|pick| pick.width > 0 && row.contains(pick.as_position())), "{picks:?} in {row:?}");
    assert!(picks.windows(2).all(|two| two[0].right() < two[1].x), "{picks:?}");
    assert_eq!((picks[0].width, picks[9].width), (13, 3));
}

#[test]
fn tabs_that_do_not_fit_drop_their_gaps_then_their_padding() {
    let names = ["Worktrees", "Agents", "Accounts", "Flows", "Issues", "UI", "Remote", "Plugins", "Machines"];

    let last = |width: u16| tabs_in(Rect::new(0, 0, width, 1), &names).last().map(|r| (r.x, r.width));

    assert_eq!([last(90), last(80), last(72)], [Some((73, 10)), Some((65, 10)), Some((57, 8))]);
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
        sections: vec![claude_usage(), codex_usage()],
        ..Usage::default()
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
    let usage = Usage { page: crate::agents::usage::Page::Limits, sections: vec![loading, failed], ..Usage::default() };
    insta::assert_snapshot!(render(&with(Overlay::Usage(usage))).backend());
}

#[rstest]
#[case::normal(0, Color::Green)]
#[case::warning(1, Color::Indexed(208))]
#[case::critical(2, Color::Red)]
fn usage_bars_take_the_severity_colour(#[case] window: u16, #[case] expected: Color) {
    let t = render(&with(Overlay::Usage(usage())));
    let body = usage_rows(usage_area(AREA, &usage()), &usage())[2];
    let bar = Position::new(body.x, body.y + 3 + window * 3);
    assert_eq!(t.backend().buffer()[bar].fg, expected);
}

#[test]
fn the_rest_of_a_usage_bar_takes_the_line_colour() {
    let t = render(&with(Overlay::Usage(usage())));
    let body = usage_rows(usage_area(AREA, &usage()), &usage())[2];
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
        sections: vec![claude_usage(), codex_usage()],
        ..Usage::default()
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
        dangerous: false,
    };
    insta::assert_snapshot!(render(&with(Overlay::Confirm(confirm))).backend());
}

#[test]
fn a_dangerous_confirmation_draws_its_submit_red() {
    let confirm = |dangerous| Confirm {
        title: "Discard changes",
        message: "Discard the changes to a.txt? This cannot be undone.".into(),
        note: None,
        submit: "discard",
        extra: None,
        dangerous,
    };
    let [submit, _] = form_buttons(form_area(AREA), "discard");
    let styled = |dangerous, hover: Option<Position>| {
        let view = View { hover, ..with(Overlay::Confirm(confirm(dangerous))) };
        let cell = render(&view).backend().buffer()[(submit.x + 1, submit.y)].clone();
        (cell.fg, cell.bg)
    };
    let off = Some(Position::new(0, 0));
    assert_eq!(styled(true, off).0, Color::Red);
    assert_eq!(styled(true, Some(submit.as_position())), (Color::Black, Color::Red));
    assert_eq!(styled(false, off).0, Color::Cyan);
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
        dangerous: false,
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

mod popup {
    use super::*;

    fn popup(text: &[u8]) -> Overlay {
        let mut emu = Emulator::new(5, 20, 0, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
        emu.feed(text);
        let screen = emu.snapshot().expect("snapshot");
        let wanted = PopupWanted { cols: Some(20), rows: Some(5) };
        Overlay::Popup(Box::new(Popup { title: "Pop: pick a branch".into(), wanted, screen }))
    }

    #[rstest]
    #[case::fits(Rect::new(0, 0, 100, 40), Some(60), Some(10), (10, 60), Rect::new(19, 14, 62, 12))]
    #[case::does_not_fit(Rect::new(0, 0, 50, 12), Some(60), Some(10), (7, 38), Rect::new(5, 1, 40, 9))]
    #[case::not_given(Rect::new(0, 0, 100, 40), None, None, (30, 78), Rect::new(10, 4, 80, 32))]
    #[case::below_the_minimum(Rect::new(0, 0, 20, 6), None, None, (5, 20), Rect::new(0, 0, 20, 6))]
    fn sizes_the_popup(
        #[case] area: Rect,
        #[case] cols: Option<u16>,
        #[case] rows: Option<u16>,
        #[case] size: (u16, u16),
        #[case] drawn: Rect,
    ) {
        let wanted = PopupWanted { cols, rows };
        assert_eq!((popup_size(area, wanted), dialog::popup::popup_area(area, wanted)), (size, drawn));
    }

    #[test]
    fn renders_the_program_inside_a_border_with_its_title_and_close() {
        insta::assert_snapshot!(render(&with(popup(b"pick one:\r\n> main"))).backend());
    }

    #[test]
    fn is_drawn_where_its_area_says_over_a_dimmed_window() {
        let view = with(popup(b"hi"));
        let corners = corners_of_its_area(&view);
        let t = render(&view);
        let inner = popup_inner(AREA, PopupWanted { cols: Some(20), rows: Some(5) });
        let buffer = t.backend().buffer();
        let behind = buffer[areas().title.as_position()].modifier.contains(Modifier::DIM);
        let inside = buffer[inner.as_position()].clone();
        assert_eq!(corners, (String::from("╭"), String::from("╯")));
        assert_eq!((behind, inside.symbol(), inside.modifier.contains(Modifier::DIM)), (true, "h", false));
    }

    #[test]
    fn its_close_lights_red_under_the_pointer() {
        let close = popup_close(AREA, PopupWanted { cols: Some(20), rows: Some(5) });
        let view = View { hover: Some(close.as_position()), ..with(popup(b"")) };
        let t = render(&view);
        let x = t.backend().buffer()[(close.x + 1, close.y)].clone();
        assert_eq!((x.symbol(), x.fg), ("×", Color::Red));
    }
}
