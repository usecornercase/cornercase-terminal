use rstest::rstest;

use super::*;

fn moving_while_deleting() -> (App, Receiver<AppEvent>, Vec<TempDir>, u64) {
    let (mut app, rx, dirs) = asked_to_delete(1, 2);
    let work = group_label(&app, 0);
    let moved = app.projects[1].id;
    click(&mut app, extra_button());
    move_to(&mut app, 1, &work);
    (app, rx, dirs, moved)
}

fn renaming_group() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    new_group(&mut app, "work");
    right_click_sidebar(&mut app, SidebarRow::Group(0));
    pick(&mut app, "rename group");
    clear_input(&mut app);
    (app, rx)
}

fn click_sidebar(app: &mut App, row: SidebarRow) {
    let pos = sidebar_pos(app, row);
    click(app, pos);
}

fn right_click_sidebar(app: &mut App, row: SidebarRow) {
    let pos = sidebar_pos(app, row);
    right_click(app, pos);
}

fn new_group(app: &mut App, name: &str) {
    let new = new_project_pos(app);
    click(app, new);
    pick(app, "new group");
    submit_text(app, name);
}

fn open_style(app: &mut App) {
    right_click_sidebar(app, SidebarRow::Group(0));
    pick(app, "icon and colour");
}

fn move_to(app: &mut App, p: usize, group: &str) {
    right_click_sidebar(app, SidebarRow::Project(p));
    pick(app, "move to group");
    pick(app, group);
}

fn group_label(app: &App, g: usize) -> String {
    app.groups[g].entry.label()
}

fn names(app: &App) -> Vec<&str> {
    app.groups.iter().map(|g| g.entry.name.as_str()).collect()
}

#[test]
fn the_new_button_offers_a_project_or_a_group() {
    let (mut app, _rx) = app();
    click(&mut app, ui::new_project_button(list(), 1, &plain(1)).as_position());
    assert_eq!(menu_labels(&app), ["open project", "new group"]);
}

#[test]
fn a_group_is_created_with_the_typed_name() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    assert_eq!((names(&app), app.overlay.is_none()), (vec!["work"], true));
}

#[test]
fn a_new_group_takes_the_first_style_without_asking() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    let entry = &app.groups[0].entry;
    assert_eq!((app.overlay.is_none(), (entry.icon, entry.colour)), (true, ui::GROUP_STYLES[0]));
}

#[test]
fn add_project_opens_the_picker_next_to_the_group_and_puts_the_project_in_it() {
    let tmp = TempDir::new();
    let root = canonical(&tmp);
    for folder in ["work/api", "work/web", "home"] {
        std::fs::create_dir_all(root.join(folder)).expect("create folder");
    }
    let (mut app, _rx) = app_in(&root.join("work/api"), no_config());
    new_group(&mut app, "work");
    let label = group_label(&app, 0);
    move_to(&mut app, 0, &label);
    app.projects.push(Project::new(999, root.join("home"), None));
    app.active = 1;

    right_click_sidebar(&mut app, SidebarRow::Group(0));
    pick(&mut app, "add project");
    let Some(Overlay::Picker { picker, .. }) = &app.overlay else { panic!("the picker is not open") };
    let starts = picker.dir().to_path_buf();
    type_text(&mut app, "web/");
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    let web = app.projects.iter().find(|p| p.path == root.join("work/web")).expect("web is open");
    assert_eq!((starts, web.group), (root.join("work"), Some(app.groups[0].id)));
}

#[test]
fn the_group_menu_offers_rename_icon_and_colour_and_delete() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    right_click_sidebar(&mut app, SidebarRow::Group(0));
    assert_eq!(menu_labels(&app), ["add project", "rename group", "icon and colour", "delete group"]);
}

#[test]
fn the_menu_button_of_a_group_opens_the_same_menu() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    let row = ui::entry_row(list(), 1, &app.sidebar_rows(), 0, SidebarRow::Group(0));
    click(&mut app, ui::row_menu_button(row, 1).as_position());
    assert_eq!(menu_labels(&app), ["add project", "rename group", "icon and colour", "delete group"]);
}

#[test]
fn clicks_in_the_modal_set_the_icon_and_the_colour() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    open_style(&mut app);

    click(&mut app, ui::style_icon(AREA, 7).as_position());
    click(&mut app, ui::style_colour(AREA, 12).as_position());

    assert_eq!((app.groups[0].entry.icon, app.groups[0].entry.colour), (ui::GROUP_ICONS[7], ui::GROUP_COLOURS[12]));
}

#[rstest]
#[case::done(None)]
#[case::enter(Some(KeyCode::Enter))]
#[case::esc(Some(KeyCode::Esc))]
fn the_modal_closes_keeping_the_choice(#[case] key: Option<KeyCode>) {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    open_style(&mut app);
    click(&mut app, ui::style_icon(AREA, 3).as_position());

    match key {
        Some(code) => send_key(&mut app, code, KeyModifiers::NONE),
        None => click(&mut app, ui::style_done(AREA).as_position()),
    }

    assert_eq!((app.overlay.is_none(), app.groups[0].entry.icon), (true, ui::GROUP_ICONS[3]));
}

#[test]
fn an_empty_name_keeps_the_form_open() {
    let (mut app, _rx) = app();
    new_group(&mut app, "  ");
    assert_eq!((app.groups.len(), matches!(app.overlay, Some(Overlay::NewGroup { .. }))), (0, true));
}

fn styles(app: &App) -> Vec<(char, u8)> {
    app.groups.iter().map(|g| (g.entry.icon, g.entry.colour)).collect()
}

#[test]
fn each_new_group_gets_the_next_style() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    new_group(&mut app, "oss");
    assert_eq!(styles(&app), ui::GROUP_STYLES[..2]);
}

#[test]
fn a_new_group_takes_a_style_no_other_group_has() {
    let (mut app, _rx) = app();
    for name in ["work", "oss", "home"] {
        new_group(&mut app, name);
    }
    let oss = app.groups[1].id;
    app.delete_group(oss);
    new_group(&mut app, "clients");
    assert_eq!(styles(&app), [ui::GROUP_STYLES[0], ui::GROUP_STYLES[2], ui::GROUP_STYLES[1]]);
}

#[test]
fn past_the_presets_the_styles_start_over() {
    let (mut app, _rx) = app();
    for i in 0..=ui::GROUP_STYLES.len() {
        new_group(&mut app, &format!("group {i}"));
    }
    assert_eq!(styles(&app)[ui::GROUP_STYLES.len()], ui::GROUP_STYLES[0]);
}

#[test]
fn the_project_menu_offers_moving_only_once_there_are_groups() {
    let (mut app, _rx) = app();
    right_click_sidebar(&mut app, SidebarRow::Project(0));
    let before = menu_labels(&app);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    new_group(&mut app, "work");
    right_click_sidebar(&mut app, SidebarRow::Project(0));
    assert_eq!(
        (before, menu_labels(&app)),
        (vec!["rename project".to_string()], vec!["rename project".to_string(), "move to group".to_string()])
    );
}

#[test]
fn a_moved_project_shows_below_its_group() {
    let (mut app, _rx, _dirs) = app_with(2);
    new_group(&mut app, "work");
    let label = group_label(&app, 0);

    move_to(&mut app, 0, &label);

    assert_eq!(
        app.sidebar_rows(),
        [SidebarRow::Project(1), SidebarRow::Gap, SidebarRow::Group(0), SidebarRow::Project(0)]
    );
}

#[test]
fn the_move_menu_lists_the_other_groups_and_no_group() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    new_group(&mut app, "oss");
    let (work, oss) = (group_label(&app, 0), group_label(&app, 1));
    move_to(&mut app, 0, &work);

    right_click_sidebar(&mut app, SidebarRow::Project(0));
    pick(&mut app, "move to group");

    assert_eq!(menu_labels(&app), [oss, "no group".to_string()]);
}

#[test]
fn no_group_takes_the_project_out() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    let work = group_label(&app, 0);
    move_to(&mut app, 0, &work);

    move_to(&mut app, 0, "no group");

    assert_eq!(app.projects[0].group, None);
}

#[test]
fn clicking_the_header_collapses_and_expands_the_group() {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");
    let work = group_label(&app, 0);
    move_to(&mut app, 0, &work);

    click_sidebar(&mut app, SidebarRow::Group(0));
    let collapsed = app.sidebar_rows();
    click_sidebar(&mut app, SidebarRow::Group(0));

    assert_eq!(
        (collapsed, app.sidebar_rows()),
        (vec![SidebarRow::Group(0)], vec![SidebarRow::Group(0), SidebarRow::Project(0)])
    );
}

#[test]
fn the_group_menu_renames_it() {
    let (mut app, _rx) = renaming_group();

    submit_text(&mut app, "clients");

    assert_eq!(names(&app), ["clients"]);
}

#[test]
fn an_empty_rename_keeps_the_group_name() {
    let (mut app, _rx) = renaming_group();

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(names(&app), ["work"]);
}

fn ask_from_the_close_button(app: &mut App) {
    let close = sidebar_close(app, SidebarRow::Group(0));
    click(app, close);
}

fn ask_from_the_menu(app: &mut App) {
    right_click_sidebar(app, SidebarRow::Group(0));
    pick(app, "delete group");
}

fn asked_to_delete_a_group_holding_a_project() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    new_group(&mut app, "work");
    let work = group_label(&app, 0);
    move_to(&mut app, 0, &work);
    ask_from_the_close_button(&mut app);
    (app, rx)
}

#[rstest]
#[case::close_button(ask_from_the_close_button)]
#[case::menu(ask_from_the_menu)]
fn deleting_a_group_asks_first(#[case] ask: fn(&mut App)) {
    let (mut app, _rx) = app();
    new_group(&mut app, "work");

    ask(&mut app);

    assert_eq!((confirmation(&app).is_some(), names(&app)), (true, vec!["work"]));
}

fn asked_to_delete(grouped: usize, projects: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(projects);
    new_group(&mut app, "work");
    let work = group_label(&app, 0);
    for p in 0..grouped {
        move_to(&mut app, p, &work);
    }
    ask_from_the_close_button(&mut app);
    (app, rx, dirs)
}

fn buttons(app: &App) -> Option<(&'static str, Option<&'static str>)> {
    match app.overlay_view(app.overlay.as_ref()?, AREA)? {
        ui::Overlay::Confirm(confirm) => Some((confirm.submit, confirm.extra)),
        _ => None,
    }
}

fn extra_button() -> Position {
    ui::form_extra_button(ui::form_area(AREA), KEEP_PROJECTS_SUBMIT, CLOSE_PROJECTS_SUBMIT).as_position()
}

#[rstest]
#[case::empty(0, "Delete the group work?")]
#[case::one_project(
    1,
    "Delete the group work? Keep its project open, ungrouped, or close it too, which stops 1 tab and the \
             programs running in it. Folders and worktrees stay on disk."
)]
#[case::two_projects(
    2,
    "Delete the group work? Keep its 2 projects open, ungrouped, or close them too, which stops 2 tabs and \
             the programs running in them. Folders and worktrees stay on disk."
)]
fn the_confirmation_says_what_happens_to_its_projects(#[case] grouped: usize, #[case] expected: &str) {
    let (app, _rx, _dirs) = asked_to_delete(grouped, 2);

    assert_eq!(confirmation(&app).as_deref(), Some(expected));
}

#[test]
fn without_tabs_the_confirmation_stops_nothing() {
    let (mut app, _rx, _dirs) = closing_last_tab();
    new_group(&mut app, "work");
    let work = group_label(&app, 0);
    move_to(&mut app, 0, &work);

    ask_from_the_close_button(&mut app);

    let expected = "Delete the group work? Keep its project open, ungrouped, or close it too. \
                Folders and worktrees stay on disk.";
    assert_eq!(confirmation(&app).as_deref(), Some(expected));
}

#[rstest]
#[case::empty(0, (DELETE_SUBMIT, None))]
#[case::with_projects(1, (KEEP_PROJECTS_SUBMIT, Some(CLOSE_PROJECTS_SUBMIT)))]
fn only_a_group_with_projects_asks_what_to_do_with_them(
    #[case] grouped: usize,
    #[case] expected: (&'static str, Option<&'static str>),
) {
    let (app, _rx, _dirs) = asked_to_delete(grouped, 1);

    assert_eq!(buttons(&app), Some(expected));
}

#[test]
fn confirming_deletes_the_group_and_keeps_its_projects_open_and_ungrouped() {
    let (mut app, _rx) = asked_to_delete_a_group_holding_a_project();

    click(&mut app, form_button(KEEP_PROJECTS_SUBMIT, 0));

    assert_eq!((app.groups.len(), app.projects.len(), app.projects[0].group), (0, 1, None));
}

#[test]
fn enter_keeps_the_projects() {
    let (mut app, _rx) = asked_to_delete_a_group_holding_a_project();

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!((app.groups.len(), app.projects.len(), app.projects[0].closing), (0, 1, false));
}

#[test]
fn closing_its_projects_deletes_the_group_and_closes_only_them() {
    let (mut app, rx, _dirs) = asked_to_delete(2, 3);
    let loose = app.projects[2].id;

    click(&mut app, extra_button());

    pump_until(&mut app, &rx, "the grouped projects close", |a| a.projects.len() == 1);
    assert_eq!(
        (app.overlay.is_none(), app.groups.len(), app.projects[0].id, app.projects[0].group),
        (true, 0, loose, None)
    );
}

#[test]
fn the_projects_being_closed_stay_in_the_group_until_they_are_gone() {
    let (mut app, rx, _dirs) = asked_to_delete(2, 2);
    let work = app.groups[0].id;

    click(&mut app, extra_button());

    let meanwhile = (app.groups.len(), app.projects.iter().map(|p| (p.closing, p.group)).collect::<Vec<_>>());
    assert_eq!(meanwhile, (1, vec![(true, Some(work)), (true, Some(work))]));
    pump_until(&mut app, &rx, "the group goes with its projects", |a| a.groups.is_empty());
    assert!(app.projects.is_empty());
}

#[test]
fn a_project_moved_into_the_group_meanwhile_keeps_it_after_the_others_close() {
    let (mut app, rx, _dirs, moved) = moving_while_deleting();

    pump_until(&mut app, &rx, "the old project closes", |a| a.projects.len() == 1);
    app.close_project(moved);
    pump_until(&mut app, &rx, "the moved project closes", App::is_empty);

    assert_eq!(app.groups.len(), 1);
}

#[test]
fn a_project_moved_into_the_group_and_closed_before_the_others_still_keeps_it() {
    let (mut app, rx, _dirs, moved) = moving_while_deleting();
    app.projects[1].workspaces[0].tabs.clear();

    app.close_project(moved);
    let closed_first = app.projects.len() == 1 && app.projects[0].closing;
    pump_until(&mut app, &rx, "the old project closes", App::is_empty);

    assert_eq!((closed_first, app.groups.len()), (true, 1));
}

#[test]
fn a_project_already_closing_does_not_count_for_the_dialog() {
    let (mut app, _rx, _dirs) = asked_to_delete(1, 1);
    app.projects[0].closing = true;

    assert_eq!(buttons(&app), Some((DELETE_SUBMIT, None)));
    assert_eq!(confirmation(&app).as_deref(), Some("Delete the group work?"));
}

#[test]
fn closing_every_project_leaves_the_app_open_and_empty() {
    let (mut app, rx, _dirs) = asked_to_delete(1, 1);

    click(&mut app, extra_button());

    pump_until(&mut app, &rx, "the project closes", App::is_empty);
    assert!(app.groups.is_empty());
}

#[test]
fn cancelling_keeps_the_group() {
    let (mut app, _rx) = asked_to_delete_a_group_holding_a_project();
    let asked = confirmation(&app).is_some();

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(
        (asked, app.overlay.is_none(), names(&app), app.projects[0].group.is_some()),
        (true, true, vec!["work"], true)
    );
}

#[test]
fn the_scroll_reveals_the_header_of_a_collapsed_group_holding_the_active_project() {
    let (mut app, _rx, _dirs) = app_with(1);
    for name in ["a", "b", "c", "d", "e", "f"] {
        new_group(&mut app, name);
    }
    let last = group_label(&app, 5);
    move_to(&mut app, 0, &last);
    app.groups[5].entry.collapsed = true;
    app.scroll.projects = 0;
    app.scroll.followed = Focus::default();

    app.follow(AREA);

    assert!(!ui::entry_row(list(), 1, &app.sidebar_rows(), app.scroll.projects, SidebarRow::Group(5)).is_empty());
}

#[test]
fn groups_survive_a_restore() {
    let (mut app, _rx, dirs) = app_with(2);
    new_group(&mut app, "work");
    new_group(&mut app, "oss");
    let oss = group_label(&app, 1);
    move_to(&mut app, 1, &oss);
    app.groups[1].entry.collapsed = true;
    let saved = app.state();

    let (mut restored, _rx2) = empty_app();
    restored.restore(&saved, AREA);

    let groups: Vec<&ui::GroupEntry> = restored.groups.iter().map(|g| &g.entry).collect();
    let expected: Vec<&ui::GroupEntry> = app.groups.iter().map(|g| &g.entry).collect();
    assert_eq!((groups, restored.sidebar_rows(), restored.state()), (expected, app.sidebar_rows(), saved));
    drop(dirs);
}
