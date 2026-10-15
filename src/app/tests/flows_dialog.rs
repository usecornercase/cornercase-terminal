use super::*;
use crate::agents::Known;
use crate::flows::dialog::{self, Dialog, Row};
use crate::flows::store;
use crate::settings::Row as SettingsRow;

const WIDE: Rect = Rect { x: 0, y: 0, width: 150, height: 44 };
const NARROW: Rect = Rect { x: 0, y: 0, width: 90, height: 30 };

struct Setup {
    app: App,
    _rx: Receiver<AppEvent>,
    config: TempDir,
    project: Option<TempDir>,
}

fn setup(project: bool) -> Setup {
    let config = TempDir::new();
    let project = project.then(TempDir::new);
    let (mut app, rx) = new_app(config.path().join("config.json"));
    if let Some(dir) = &project {
        app.open_project(dir.path().to_path_buf(), WIDE).expect("open project");
    }
    for kind in Known::ALL {
        app.config.agent_commands.insert(kind.name().into(), "/nonexistent/agent".into());
    }
    Setup { app, _rx: rx, config, project }
}

fn open(project: bool) -> Setup {
    let mut s = setup(project);
    s.app.open_flows(None, None);
    s
}

fn dialog(app: &App) -> &Dialog {
    let Some(Overlay::Flows { dialog, .. }) = &app.overlay else { panic!("the flows dialog is not open") };
    dialog
}

fn view(app: &App, area: Rect) -> ui::Flows {
    let ui::Overlay::Flows(view) = overlay_shown(app, area) else { panic!("the flows dialog is not shown") };
    *view
}

pub(super) fn button(app: &mut App, area: Rect, label: &str) {
    let view = view(app, area);
    click_button(app, area, &view.buttons, &ui::flows_buttons(area, view.warnings.len(), &view.buttons), label);
}

fn row_rect(app: &App, area: Rect, i: usize) -> Rect {
    let view = view(app, area);
    let sections: Vec<&str> = view.rows.iter().map(|r| r.section).collect();
    let body = ui::flows_body(area, view.warnings.len(), view.preview.is_some());
    ui::line_rect(body, &sections, view.cursor, i)
}

fn click_list_row(app: &mut App, area: Rect, i: usize) {
    let pos = row_rect(app, area, i).as_position();
    click_in(app, pos, area);
}

fn click_row(app: &mut App, area: Rect, row: Row) {
    let edit = dialog(app).edit.as_deref().expect("the edit page is open");
    let i = edit.rows().iter().position(|r| *r == row).expect("the row is shown");
    click_list_row(app, area, i);
}

fn tab(app: &mut App, area: Rect, i: usize) {
    let view = view(app, area);
    let names: Vec<&str> = view.tabs.iter().map(String::as_str).collect();
    click_in(app, ui::flows_tabs(area, &names)[i].as_position(), area);
}

fn key_in(app: &mut App, code: KeyCode, area: Rect) {
    app.handle_event(AppEvent::Input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))), area).expect("key");
}

fn type_in(app: &mut App, text: &str, area: Rect) {
    for c in text.chars() {
        key_in(app, KeyCode::Char(c), area);
    }
}

fn user_dir(s: &Setup) -> PathBuf {
    paths::flows(&s.config.path().join("config.json"))
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

fn select_flow(app: &mut App, area: Rect, name: &str) {
    let i = dialog(app).flows.iter().position(|saved| saved.flow.name == name).expect("the flow is listed");
    if dialog(app).cursor != i {
        click_list_row(app, area, i);
    }
}

fn duplicate_full_issue(s: &mut Setup, area: Rect) {
    select_flow(&mut s.app, area, "Full issue");
    button(&mut s.app, area, dialog::DUPLICATE);
}

fn coder_prompt() -> Setup {
    let mut s = open(false);
    duplicate_full_issue(&mut s, WIDE);
    tab(&mut s.app, WIDE, 2);
    click_row(&mut s.app, WIDE, Row::Prompt);
    s
}

fn shown(app: &mut App, area: Rect) -> String {
    format!("{}", render(app, area).backend())
}

mod opening {
    use super::*;

    #[test]
    fn the_settings_row_opens_the_dialog_and_done_comes_back() {
        let mut s = setup(false);
        s.app.open_settings_on(&SettingsRow::Flows);
        let Some(Overlay::Settings(settings)) = &s.app.overlay else { panic!("no settings") };
        assert_eq!(settings.flows, "4 built in");
        let i = settings.rows().iter().position(|r| *r == SettingsRow::Flows).expect("the flows row");
        let sections: Vec<&'static str> = settings.rows().iter().map(SettingsRow::section).collect();
        let pos = ui::settings_row(WIDE, &sections, settings.cursor, i).as_position();
        click_in(&mut s.app, pos, WIDE);
        assert_eq!(dialog(&s.app).entries(), 4);
        duplicate_full_issue(&mut s, WIDE);
        button(&mut s.app, WIDE, dialog::DONE);
        button(&mut s.app, WIDE, dialog::DONE);
        let Some(Overlay::Settings(settings)) = &s.app.overlay else { panic!("the settings did not come back") };
        assert_eq!(settings.flows, "4 built in, 1 yours");
    }

    #[test]
    fn a_flow_named_by_its_file_opens_on_its_edit_page() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        s.app.overlay = None;
        let path = user_dir(&s).join("full-issue-copy.toml");
        s.app.open_flows(None, Some((&store::Origin::User(path), "")));
        assert_eq!(dialog(&s.app).edit.as_ref().map(|e| e.flow.name.as_str()), Some("Full issue copy"));
    }
}

mod files_on_disk {
    use super::*;

    #[test]
    fn duplicating_a_built_in_flow_saves_a_copy_in_your_folder_and_opens_it() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        assert_eq!(files(&user_dir(&s)), ["full-issue-copy.toml"]);
        let edit = dialog(&s.app).edit.as_deref().expect("the copy is open");
        assert_eq!((edit.flow.name.as_str(), edit.dirty()), ("Full issue copy", false));
    }

    #[test]
    fn renaming_and_saving_moves_the_file() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        click_row(&mut s.app, WIDE, Row::Name);
        for _ in 0.."copy".len() {
            key_in(&mut s.app, KeyCode::Backspace, WIDE);
        }
        type_in(&mut s.app, "again", WIDE);
        button(&mut s.app, WIDE, dialog::SAVE);
        button(&mut s.app, WIDE, dialog::SAVE);
        assert_eq!(files(&user_dir(&s)), ["full-issue-again.toml"]);
        let saved = store::read(&user_dir(&s).join("full-issue-again.toml")).expect("read the flow");
        assert_eq!(saved.name, "Full issue again");
    }

    #[test]
    fn deleting_asks_then_removes_the_file() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        button(&mut s.app, WIDE, dialog::DONE);
        let copy = dialog(&s.app).flows.iter().position(|f| f.flow.name == "Full issue copy");
        assert_eq!(Some(dialog(&s.app).cursor), copy);
        button(&mut s.app, WIDE, dialog::DELETE);
        assert_eq!(files(&user_dir(&s)), ["full-issue-copy.toml"]);
        let [submit, _] = ui::form_buttons(ui::form_area(WIDE), dialog::DELETE);
        click_in(&mut s.app, submit.as_position(), WIDE);
        assert_eq!(files(&user_dir(&s)), Vec::<String>::new());
        assert_eq!(dialog(&s.app).entries(), 4);
    }

    #[test]
    fn a_flow_can_be_saved_into_the_project() {
        let mut s = open(true);
        button(&mut s.app, WIDE, dialog::NEW);
        click_row(&mut s.app, WIDE, Row::Place);
        let list = ui::pick_list_in(ui::flows_body(WIDE, 1, true));
        click_in(&mut s.app, Position::new(list.x + 2, list.y + 1), WIDE);
        button(&mut s.app, WIDE, dialog::SAVE);
        let project = s.project.as_ref().expect("a project").path();
        assert_eq!(files(&store::project_dir(project)), ["new-flow.toml"]);
        assert_eq!(files(&user_dir(&s)), Vec::<String>::new());
    }

    #[test]
    fn a_flow_with_problems_is_not_written() {
        let mut s = open(false);
        button(&mut s.app, WIDE, dialog::NEW);
        tab(&mut s.app, WIDE, 2);
        button(&mut s.app, WIDE, dialog::DELETE_ROLE);
        tab(&mut s.app, WIDE, 2);
        button(&mut s.app, WIDE, dialog::DELETE_ROLE);
        button(&mut s.app, WIDE, dialog::SAVE);
        assert_eq!(files(&user_dir(&s)), Vec::<String>::new());
        assert!(view(&s.app, WIDE).warnings.contains(&"the flow has no stages".to_string()));
    }
}

mod built_in_flows {
    use super::*;

    fn origin_of(app: &App, name: &str) -> store::Origin {
        let named: Vec<&store::Saved> = dialog(app).flows.iter().filter(|saved| saved.flow.name == name).collect();
        assert_eq!(named.len(), 1, "one flow is named {name}");
        named[0].origin.clone()
    }

    #[test]
    fn saving_one_puts_your_version_in_its_place_and_restore_default_brings_it_back() {
        let mut s = open(false);
        select_flow(&mut s.app, WIDE, "Full issue");
        button(&mut s.app, WIDE, dialog::EDIT);
        click_row(&mut s.app, WIDE, Row::Notes);
        type_in(&mut s.app, "Keep the API.", WIDE);
        button(&mut s.app, WIDE, dialog::SAVE);
        button(&mut s.app, WIDE, dialog::SAVE);
        let file = user_dir(&s).join("full-issue.toml");
        assert_eq!(files(&user_dir(&s)), ["full-issue.toml"]);
        assert_eq!(store::read(&file).expect("read your version").notes, "Keep the API.");
        button(&mut s.app, WIDE, dialog::DONE);
        assert_eq!((dialog(&s.app).entries(), origin_of(&s.app, "Full issue")), (4, store::Origin::User(file)));
        let note = view(&s.app, WIDE).rows[dialog(&s.app).cursor].note.clone();
        assert!(note.starts_with("edited · "), "{note}");

        button(&mut s.app, WIDE, dialog::RESTORE);
        let [submit, _] = ui::form_buttons(ui::form_area(WIDE), dialog::RESTORE);
        click_in(&mut s.app, submit.as_position(), WIDE);
        assert_eq!(files(&user_dir(&s)), Vec::<String>::new());
        assert_eq!(origin_of(&s.app, "Full issue"), store::Origin::BuiltIn);
    }
}

mod preview {
    use super::*;

    const SHORT: Rect = Rect { x: 0, y: 0, width: 150, height: 28 };

    fn preview_scroll(app: &App) -> u16 {
        dialog(app).edit.as_ref().map(|edit| edit.preview).expect("the edit page is open")
    }

    fn body(app: &App) -> Rect {
        let warnings = view(app, SHORT).warnings.len();
        ui::flows_preview(SHORT, warnings, true, false).map(ui::preview_body).expect("the preview beside the form")
    }

    fn wheel(app: &mut App, kind: MouseEventKind, times: usize) {
        for _ in 0..times {
            let at = body(app).as_position();
            mouse_in(app, kind, at, SHORT);
        }
    }

    #[test]
    fn the_wheel_scrolls_it_and_its_edges_say_there_is_more() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, SHORT);
        assert_eq!(preview_scroll(&s.app), 0);
        assert!(shown(&mut s.app, SHORT).contains(" ↓ more "));
        wheel(&mut s.app, MouseEventKind::ScrollDown, 1);
        assert!(preview_scroll(&s.app) > 0);
        let screen = shown(&mut s.app, SHORT);
        assert!(screen.contains(" ↑ more ") && screen.contains(" ↓ more "), "{screen}");
        wheel(&mut s.app, MouseEventKind::ScrollDown, 40);
        let screen = shown(&mut s.app, SHORT);
        assert!(screen.contains(" ↑ more ") && !screen.contains(" ↓ more "), "{screen}");
        wheel(&mut s.app, MouseEventKind::ScrollUp, 40);
        assert_eq!(preview_scroll(&s.app), 0);
    }

    #[test]
    fn opening_a_stage_scrolls_to_its_boxes_and_draws_them_heavy() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, SHORT);
        assert!(!shown(&mut s.app, SHORT).contains("┏━ tester"));
        click_row(&mut s.app, SHORT, Row::Stage(2));
        assert!(preview_scroll(&s.app) > 0);
        let screen = shown(&mut s.app, SHORT);
        assert!(screen.contains("┏━ tester") && screen.contains(" ↑ more "), "{screen}");
        assert!(!screen.contains("┏━ coder"), "{screen}");
    }
}

mod leaving {
    use super::*;

    #[test]
    fn a_press_outside_with_unsaved_changes_asks_first() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        tab(&mut s.app, WIDE, 3);
        click_row(&mut s.app, WIDE, Row::Edits);
        click_in(&mut s.app, Position::new(0, 0), WIDE);
        assert_eq!(dialog(&s.app).confirm, Some(dialog::Confirm::Leave(dialog::Then::Close)));
        click_in(&mut s.app, Position::new(0, 0), WIDE);
        assert!(dialog(&s.app).dirty());
        click_in(&mut s.app, Position::new(0, 0), WIDE);
        let [discard, _] = ui::form_buttons(ui::form_area(WIDE), dialog::DISCARD);
        click_in(&mut s.app, discard.as_position(), WIDE);
        assert!(s.app.overlay.is_none());
    }

    #[test]
    fn a_press_outside_without_changes_closes_the_whole_dialog() {
        let mut s = open(false);
        click_in(&mut s.app, Position::new(0, 0), WIDE);
        assert!(s.app.overlay.is_none());
    }
}

mod long_texts {
    use super::*;

    #[test]
    fn a_prompt_takes_typed_and_pasted_lines() {
        let mut s = coder_prompt();
        type_in(&mut s.app, "Intro", WIDE);
        key_in(&mut s.app, KeyCode::Enter, WIDE);
        s.app.handle_event(AppEvent::Input(Event::Paste("first\nsecond\n".into())), WIDE).expect("paste");
        button(&mut s.app, WIDE, dialog::SAVE);
        button(&mut s.app, WIDE, dialog::SAVE);
        let saved = store::read(&user_dir(&s).join("full-issue-copy.toml")).expect("read the flow");
        assert!(
            saved.roles[0].prompt.starts_with("Intro\nfirst\nsecond\nYou are the coder"),
            "{}",
            saved.roles[0].prompt
        );
    }

    #[test]
    fn a_click_in_the_text_places_the_cursor() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        click_row(&mut s.app, WIDE, Row::Notes);
        s.app.handle_event(AppEvent::Input(Event::Paste("abc\ndef".into())), WIDE).expect("paste");
        let text = ui::flows_text(WIDE, 1, false);
        click_in(&mut s.app, Position::new(text.x + 1, text.y), WIDE);
        type_in(&mut s.app, "X", WIDE);
        button(&mut s.app, WIDE, dialog::SAVE);
        assert_eq!(dialog(&s.app).edit.as_ref().map(|e| e.flow.notes.as_str()), Some("aXbc\ndef"));
    }
}

mod models {
    use super::*;

    #[test]
    fn models_read_on_a_job_fill_the_open_pick() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        tab(&mut s.app, WIDE, 2);
        let Some(Overlay::Flows { dialog, .. }) = &mut s.app.overlay else { panic!("no dialog") };
        dialog.edit.as_deref_mut().expect("edit").flow.roles[0].agent = "opencode".into();
        s.app.handle_event(AppEvent::FlowModels(Known::Opencode, vec!["deepseek/v4".into()]), WIDE).expect("event");
        click_row(&mut s.app, WIDE, Row::Model);
        let pick = view(&s.app, WIDE).pick.expect("the model pick");
        let labels: Vec<&str> = pick.items.iter().map(|(label, _, _)| label.as_str()).collect();
        assert_eq!(labels, ["default", "deepseek/v4", "other…"]);
    }
}

mod pages {
    use super::*;

    #[rstest::rstest]
    #[case::wide(WIDE, "wide")]
    #[case::narrow(NARROW, "narrow")]
    fn the_list(#[case] area: Rect, #[case] size: &str) {
        let mut s = open(false);
        insta::assert_snapshot!(format!("the_list_{size}"), shown(&mut s.app, area));
    }

    #[rstest::rstest]
    #[case::wide(WIDE, "wide")]
    #[case::narrow(NARROW, "narrow")]
    fn the_flow_tab_with_a_stage_open(#[case] area: Rect, #[case] size: &str) {
        let mut s = open(false);
        duplicate_full_issue(&mut s, area);
        click_row(&mut s.app, area, Row::Stage(1));
        insta::assert_snapshot!(format!("the_flow_tab_with_a_stage_open_{size}"), shown(&mut s.app, area));
    }

    #[rstest::rstest]
    #[case::wide(WIDE, "wide")]
    #[case::narrow(NARROW, "narrow")]
    fn a_role_tab(#[case] area: Rect, #[case] size: &str) {
        let mut s = open(false);
        duplicate_full_issue(&mut s, area);
        tab(&mut s.app, area, 3);
        insta::assert_snapshot!(format!("a_role_tab_{size}"), shown(&mut s.app, area));
    }

    #[test]
    fn the_diagram_behind_its_button() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, NARROW);
        button(&mut s.app, NARROW, dialog::DIAGRAM);
        insta::assert_snapshot!(shown(&mut s.app, NARROW));
    }

    #[test]
    fn the_prompt_editor() {
        let mut s = coder_prompt();
        insta::assert_snapshot!(shown(&mut s.app, WIDE));
    }

    #[test]
    fn the_stage_roles_pick() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        click_row(&mut s.app, WIDE, Row::Stage(1));
        click_row(&mut s.app, WIDE, Row::Roles(1));
        insta::assert_snapshot!(shown(&mut s.app, WIDE));
    }

    #[test]
    fn a_flow_with_problems() {
        let mut s = open(false);
        duplicate_full_issue(&mut s, WIDE);
        click_row(&mut s.app, WIDE, Row::Stage(0));
        let remove = ui::settings_remove(row_rect(&s.app, WIDE, 3));
        click_in(&mut s.app, remove.as_position(), WIDE);
        insta::assert_snapshot!(shown(&mut s.app, WIDE));
    }
}
