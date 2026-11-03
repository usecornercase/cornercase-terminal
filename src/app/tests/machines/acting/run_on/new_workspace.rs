use super::*;
use crate::machines::run_on::RunOn;

const MADE: u64 = 12;

fn created(command: &str) -> Structured {
    match command {
        "new-workspace" => Structured::Ok(json!({ "workspace": MADE })),
        command => ready(command),
    }
}

fn refused(command: &str) -> Structured {
    match command {
        "new-workspace" => {
            Structured::Error(Refusal::new(Code::Failed, "the folder on vps.lan is not a git repository"))
        }
        command => ready(command),
    }
}

fn asking(told: fn(&str) -> Structured, projects: Vec<ProjectInfo>) -> Reading {
    let mut reading = reading_with(Reply::Told(told), projects);
    let app = &mut reading.remote.rig.app;
    app.overlay = None;
    app.ask_new_workspace(0);
    reading
}

fn form(app: &App) -> ui::Form {
    let ui::Overlay::Form(form) = overlay_shown(app, AREA) else { panic!("New workspace is not shown") };
    form
}

fn chosen(app: &App) -> Option<String> {
    let Some(Overlay::NewWorkspace { run_on, .. }) = &app.overlay else { panic!("New workspace is not open") };
    run_on.as_ref().and_then(RunOn::machine).map(|target| target.name.clone())
}

impl Reading {
    fn new_on_vps(&mut self) {
        let app = &mut self.remote.rig.app;
        let row = ui::form_run_on(ui::form_area(AREA));
        click(app, Position::new(row.x + 30, row.y));
    }

    fn create(&mut self, name: &str) {
        let app = &mut self.remote.rig.app;
        type_text(app, name);
        send_key(app, KeyCode::Enter, KeyModifiers::NONE);
    }

    fn created(&mut self) {
        self.remote.rig.until("the machine answers", |app| match &app.overlay {
            Some(Overlay::NewWorkspace { creating, .. }) => !creating,
            _ => true,
        });
    }
}

#[test]
fn sends_the_workspace_to_the_machine_and_selects_it_once_shown() {
    let mut reading = asking(created, vec![web_shop()]);
    reading.new_on_vps();

    reading.create("login");

    let starting = Some(ui::Note::Info("starting on vps…".into()));
    assert_eq!(form(&reading.remote.rig.app).note, starting);
    reading.created();
    let sent = reading.sent("new-workspace").expect("a new workspace");
    assert_eq!((&sent["name"], &sent["project"], &sent["worktree"]), (&json!("login"), &json!(1), &json!(false)));
    assert_eq!((&sent["focus"], sent.get("machine")), (&json!(false), None));
    assert!(reading.remote.rig.app.overlay.is_none());
    let made = WorkspaceInfo { id: MADE, name: "login".into(), ..workspace() };
    serve(&mut reading.remote.theirs, 2, shop(vec![ProjectInfo { workspaces: vec![workspace(), made], ..web_shop() }]));
    reading
        .remote
        .rig
        .until("the new workspace is selected", |app| app.shown_remote().is_some_and(|shown| shown.workspace == MADE));
}

#[test]
fn keeps_the_machine_s_refusal_in_red() {
    let mut reading = asking(refused, vec![web_shop()]);
    reading.new_on_vps();

    reading.create("login");
    reading.created();

    let error = Some(ui::Note::Error("the folder on vps is not a git repository".into()));
    assert_eq!(form(&reading.remote.rig.app).note, error);
}

#[test]
fn creates_nothing_here_once_the_chosen_machine_is_gone() {
    let mut reading = asking(created, vec![web_shop()]);
    reading.new_on_vps();
    reading.remote.rig.edit(Vec::new(), Duration::from_secs(3));
    reading.remote.rig.app.follow_new_workspace_run_on();

    reading.create("login");

    let app = &reading.remote.rig.app;
    assert_eq!(form(app).note, Some(ui::Note::Error("vps is no longer listed".into())));
    assert_eq!(chosen(app), None);
}

#[test]
fn asks_for_a_name_before_sending() {
    let mut reading = asking(created, vec![web_shop()]);
    reading.new_on_vps();

    reading.create("");

    assert_eq!(form(&reading.remote.rig.app).note, Some(ui::Note::Error("the name is required".into())));
    assert_eq!(reading.sent("new-workspace"), None);
}

#[test]
fn cannot_choose_a_machine_without_a_git_remote() {
    let mut reading = reading_with(Reply::Told(created), vec![web_shop()]);
    let app = &mut reading.remote.rig.app;
    app.projects[0].repo.identity = None;
    app.overlay = None;
    app.ask_new_workspace(0);

    reading.new_on_vps();

    let app = &reading.remote.rig.app;
    assert_eq!(chosen(app), None);
    let spans = form(app).run_on.expect("the run on row").spans;
    let row: String = spans.iter().map(|span| span.content.as_ref()).collect();
    assert!(row.ends_with(" — needs a git remote"), "{row}");
}

#[test]
fn opens_the_clone_dialog_when_the_project_is_not_there() {
    let mut reading = asking(created, Vec::new());
    reading.remote.rig.app.projects[0].repo.remotes =
        vec![("origin".into(), "https://github.com/acme/web-shop.git".into())];
    reading.new_on_vps();
    assert_eq!(form(&reading.remote.rig.app).hint, "not on vps · clone…");

    reading.create("login");

    assert!(matches!(reading.remote.rig.app.overlay, Some(Overlay::CloneOnMachine(_))));
    assert_eq!(reading.sent("new-workspace"), None);
}

#[test]
fn shows_where_it_creates_it_on_the_machine() {
    let mut reading = asking(created, vec![web_shop()]);

    reading.new_on_vps();

    assert_eq!(chosen(&reading.remote.rig.app).as_deref(), Some("vps"));
    insta::assert_snapshot!(dialog(&mut reading.remote.rig.app, AREA));
}

#[test]
fn leaves_the_dialog_waiting_when_an_earlier_dialog_s_answer_arrives() {
    let mut reading = asking(created, vec![web_shop()]);
    reading.new_on_vps();
    let app = &mut reading.remote.rig.app;
    let link = app.machines.held().iter().find(|held| held.name == "vps").expect("vps").link.id();
    if let Some(Overlay::NewWorkspace { run_on: Some(run_on), creating, .. }) = &mut app.overlay {
        run_on.awaiting = Some(2);
        *creating = true;
    }

    app.dialog_relayed(link, 1, Err(Refusal::new(Code::Failed, "the earlier one failed")));
    assert!(
        matches!(&app.overlay, Some(Overlay::NewWorkspace { creating: true, error: None, .. })),
        "{:?}",
        app.overlay
    );

    app.dialog_relayed(link, 2, Err(Refusal::new(Code::Failed, "this one failed")));
    let Some(Overlay::NewWorkspace { creating, error, .. }) = &app.overlay else { panic!("New workspace is not open") };
    assert_eq!((*creating, error.as_deref()), (false, Some("this one failed")));
}
