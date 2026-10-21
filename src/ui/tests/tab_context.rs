use super::*;

fn opus(percent: u16) -> Details {
    Details { model: Some("Opus 5.5".into()), percent: Some(percent), subagents: 0, memory: None, account: None }
}

fn with_details(details: Details) -> View<'static> {
    let claude = TabEntry { status: Some(Status::Working), details, ..TabEntry::from("claude") };
    View {
        has_project: true,
        workspaces: vec![WorkspaceEntry {
            collapsed: false,
            name: "login".into(),
            tabs: vec![claude, "nvim".into()],
            behind: 0,
            pull: None,
            removing: false,
            setting_up: false,
            closed: 0,
        }],
        active_tab: Some(0),
        ..view(&["shop"])
    }
}

fn row(v: &View, area: Rect, row: WorkspaceRow) -> Rect {
    let a = layout(area, v.widths);
    workspace_row(a.workspaces_list, a.pitch, &v.tab_lines(), 0, row)
}

fn line(t: &Terminal<TestBackend>, r: Rect, y: u16) -> String {
    row_text(t, Rect { y: r.y + y, height: 1, ..r }).trim_end().to_string()
}

#[test]
fn renders_the_model_and_the_context_under_the_tab() {
    insta::assert_snapshot!(render(&with_details(opus(17))).backend());
}

fn agent(name: &str, model: &str, percent: Option<u16>) -> View<'static> {
    let mut view =
        with_details(Details { model: Some(model.into()), percent, subagents: 0, memory: None, account: None });
    let tab = &mut view.workspaces[0].tabs[0];
    tab.name = name.into();
    tab.status = None;
    view
}

fn codex(model: &str, percent: Option<u16>) -> View<'static> {
    agent("codex", model, percent)
}

#[test]
fn renders_codex_context_in_the_wide_layout() {
    insta::assert_snapshot!(render(&codex("gpt-5.4", Some(20))).backend());
}

#[test]
fn renders_opencode_working_with_its_model_and_context() {
    let mut view = agent("opencode", "DeepSeek V4 Pro", Some(12));
    view.workspaces[0].tabs[0].status = Some(Status::Working);

    insta::assert_snapshot!(render(&view).backend());
}

#[test]
fn renders_codex_without_a_percentage_in_the_compact_layout() {
    let view = View { nav: Some(Nav::Workspaces), ..codex("gpt-5.4-mini", None) };

    insta::assert_snapshot!(render_sized(&view, SMALL.width, SMALL.height).backend());
}

#[rstest]
#[case::wide(AREA)]
#[case::compact(SMALL)]
#[case::narrow(Rect::new(0, 0, 30, 25))]
fn long_codex_model_names_are_truncated_and_keep_the_percentage(#[case] area: Rect) {
    let model = "a-very-long-codex-model-name".repeat(5);
    let view = View { nav: Some(Nav::Workspaces), ..codex(&model, Some(91)) };
    let r = row(&view, area, WorkspaceRow::Tab(0, 0));
    let rendered = render_sized(&view, area.width, area.height);
    let text = line(&rendered, r, r.height - 1);

    assert!(text.contains("91%"), "{text}");
    assert!(!text.contains(&model), "{text}");
}

#[test]
fn a_model_without_usage_is_truncated_in_the_wide_layout() {
    let view = codex("a-very-long-codex-model-name", None);
    let r = row(&view, AREA, WorkspaceRow::Tab(0, 0));
    let rendered = render(&view);
    let text = line(&rendered, r, 1);

    assert!(text.contains('…'), "{text}");
    assert!(!text.contains('%') && !text.contains('·'), "{text}");
}

#[test]
fn the_tab_takes_a_second_row() {
    assert_eq!(row(&with_details(opus(17)), AREA, WorkspaceRow::Tab(0, 0)).height, 2);
}

#[test]
fn the_rows_below_move_down_one() {
    let moved = row(&with_details(opus(17)), AREA, WorkspaceRow::Tab(0, 1));
    let before = row(&with_details(Details::default()), AREA, WorkspaceRow::Tab(0, 1));
    assert_eq!(moved.y, before.y + 1);
}

#[rstest]
#[case::fits(26, None, "▌ │   Opus 5.5 · 17%")]
#[case::cuts_the_model(19, None, "▌ │   Opus… · 17%")]
#[case::keeps_the_percentage(16, None, "▌ │   17%")]
#[case::fits_with_the_memory(32, Some(GB * 12 / 10), "▌ │   Opus 5.5 · 17% · 1.2 GB")]
#[case::cuts_the_model_before_the_memory(30, Some(GB * 12 / 10), "▌ │   Opus 5… · 17% · 1.2 GB")]
#[case::drops_the_model_before_the_memory(26, Some(GB * 12 / 10), "▌ │   17% · 1.2 GB")]
#[case::moves_left_to_keep_the_memory(16, Some(GB * 12 / 10), "▌ 17% · 1.2 GB")]
fn a_narrow_column_cuts_the_model_first(#[case] workspaces: u16, #[case] memory: Option<u64>, #[case] expected: &str) {
    let v = View { widths: Widths { workspaces, ..Widths::default() }, ..with_details(Details { memory, ..opus(17) }) };
    let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));
    assert_eq!(line(&render(&v), r, 1), expected);
}

#[rstest]
#[case::the_model(Details { model: Some("Opus 5.5".into()), ..Details::default() }, "▌ │   Opus 5.5")]
#[case::the_context(Details { percent: Some(17), ..Details::default() }, "▌ │   17%")]
#[case::the_memory(Details { memory: Some(300 * MB), ..Details::default() }, "▌ │   300 MB")]
#[case::the_model_and_the_memory(Details { memory: Some(GB * 12 / 10), ..codex_model() }, "▌ │   gpt-5.4 · 1.2 GB")]
#[case::the_context_and_the_memory(Details { model: None, memory: Some(300 * MB), ..opus(17) }, "▌ │   17% · 300 MB")]
#[case::one_subagent(Details { subagents: 1, ..Details::default() }, "▌ │   1 subagent")]
#[case::the_context_and_the_subagents(Details { subagents: 2, ..opus(17) }, "▌ │   17% · 2 subagents")]
#[case::the_subagents_and_the_memory(Details { subagents: 2, memory: Some(300 * MB), ..Details::default() }, "▌ │ 2 subagents · 300 MB")]
fn each_part_shows_without_the_others(#[case] details: Details, #[case] expected: &str) {
    let v = with_details(details);
    let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));

    assert_eq!((r.height, line(&render(&v), r, 1)), (2, expected.into()));
}

fn codex_model() -> Details {
    Details { model: Some("gpt-5.4".into()), ..Details::default() }
}

#[test]
fn renders_the_memory_in_the_compact_layout() {
    let view = View { nav: Some(Nav::Workspaces), ..with_details(Details { memory: Some(GB * 12 / 10), ..opus(17) }) };

    insta::assert_snapshot!(render_sized(&view, SMALL.width, SMALL.height).backend());
}

#[rstest]
#[case::megabytes(300 * MB, "300 MB")]
#[case::rounded_to_the_nearest_megabyte(300 * MB + MB * 6 / 10, "301 MB")]
#[case::nearly_a_gigabyte(GB - MB / 10, "1.0 GB")]
#[case::gigabytes(GB * 12 / 10, "1.2 GB")]
#[case::rounded_to_a_tenth(GB * 25 / 2, "12.5 GB")]
fn memory_reads_in_megabytes_then_gigabytes(#[case] bytes: u64, #[case] expected: &str) {
    assert_eq!(memory_size(bytes), expected);
}

#[rstest]
#[case::normal(40, Color::DarkGray)]
#[case::getting_full(80, WAITING_COLOR)]
#[case::nearly_full(95, Color::Red)]
fn the_percentage_turns_orange_then_red(#[case] percent: u16, #[case] colour: Color) {
    let v = with_details(opus(percent));
    let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));
    let t = render(&v);
    let end = u16::try_from(line(&t, r, 1).chars().count()).expect("fits");
    assert_eq!(t.backend().buffer()[(r.x + end - 1, r.y + 1)].fg, colour);
}

#[test]
fn only_the_first_row_has_the_close_button() {
    let v = with_details(opus(17));
    let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));
    let hit =
        |x: u16, y: u16| workspace_hit(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, Position::new(x, y));
    let menu = row_menu_button(r, areas().pitch);
    assert_eq!(
        [hit(r.x + 4, r.y + 1), hit(r.right() - 2, r.y + 1), hit(r.right() - 2, r.y), hit(menu.x, menu.y)],
        [
            Some(WorkspaceHit::Tab(0, 0)),
            Some(WorkspaceHit::Tab(0, 0)),
            Some(WorkspaceHit::CloseTab(0, 0)),
            Some(WorkspaceHit::TabMenu(0, 0))
        ]
    );
}

#[test]
fn the_close_button_covers_only_the_first_band_of_a_taller_row() {
    assert_eq!(row_close_button(Rect::new(0, 4, 25, 2), 1), Rect::new(22, 4, CLOSE_BUTTON_WIDTH, 1));
}

#[test]
fn a_narrow_compact_band_keeps_the_percentage_clear_of_the_buttons() {
    let narrow = Rect { width: 34, ..SMALL };
    let details = Details { model: Some("Opus 5.5 with a long name".into()), ..opus(95) };
    let v = View { nav: Some(Nav::Workspaces), ..with_details(details) };
    let r = row(&v, narrow, WorkspaceRow::Tab(0, 0));
    let t = render_sized(&v, narrow.width, narrow.height);
    let end = u16::try_from(line(&t, r, 2).chars().count()).expect("fits");
    assert!(r.x + end <= row_menu_button(r, COMPACT_PITCH).x, "{}", line(&t, r, 2));
}

#[test]
fn a_compact_band_shows_it_on_its_last_row() {
    let v = View { nav: Some(Nav::Workspaces), ..with_details(opus(17)) };
    let r = row(&v, SMALL, WorkspaceRow::Tab(0, 0));
    let t = render_sized(&v, SMALL.width, SMALL.height);
    assert_eq!(
        (r.height, line(&t, r, 1), line(&t, r, 2)),
        (
            COMPACT_PITCH,
            format!("▌ ├ ◐ claude{}{ROW_MENU_ICON}  ×", " ".repeat(usize::from(SMALL.width) - 18)),
            "▌ │   Opus 5.5 · 17%".into()
        )
    );
}

fn work(details: Details) -> Details {
    Details { account: Some("work".into()), ..details }
}

#[test]
fn renders_the_account_after_the_model() {
    insta::assert_snapshot!(render(&with_details(work(opus(17)))).backend());
}

#[test]
fn renders_the_account_in_the_compact_layout() {
    let view = View { nav: Some(Nav::Workspaces), ..with_details(work(opus(17))) };

    insta::assert_snapshot!(render_sized(&view, SMALL.width, SMALL.height).backend());
}

#[test]
fn the_account_alone_takes_a_second_row() {
    assert_eq!(work(Details::default()).lines(), 2);
}

#[rstest]
#[case::fits(34, "work", "▌ │   Opus 5.5 · work · 17%")]
#[case::cuts_the_model_first(26, "work", "▌ │   Opus… · work · 17%")]
#[case::drops_the_model_first(24, "personal", "▌ │   personal · 17%")]
#[case::cuts_the_account(19, "personal", "▌ │   pers… · 17%")]
#[case::keeps_three_cells_of_the_account(17, "personal", "▌ │   pe… · 17%")]
#[case::drops_the_account_below_three_cells(16, "personal", "▌ │   17%")]
fn a_narrow_column_cuts_the_model_then_the_account(
    #[case] workspaces: u16,
    #[case] account: &str,
    #[case] expected: &str,
) {
    let details = Details { account: Some(account.into()), ..opus(17) };
    let v = View { widths: Widths { workspaces, ..Widths::default() }, ..with_details(details) };
    let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));
    assert_eq!(line(&render(&v), r, 1), expected);
}
