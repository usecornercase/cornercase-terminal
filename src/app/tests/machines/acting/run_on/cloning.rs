use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;
use crate::app::clone_on_machine::failure;
use crate::error::Error;
use crate::machines::clone_form::{BUTTONS, CloneForm, Files};
use crate::test_util::{git, wait_until};

const URL: &str = "https://github.com/acme/web-shop.git";
const CLONED: u64 = 2;

fn cloned(command: &str) -> Structured {
    match command {
        "clone" => Structured::Ok(json!({ "path": "/home/ana/code/web-shop", "project": CLONED })),
        command => ready(command),
    }
}

fn half_copied(command: &str) -> Structured {
    match command {
        "clone" => Structured::Ok(json!({
            "path": "/home/ana/code/web-shop",
            "project": CLONED,
            "unwritten": [{ "path": "local.toml", "why": "it is tracked there" }],
        })),
        command => ready(command),
    }
}

fn refused(command: &str) -> Structured {
    match command {
        "clone" => Structured::Error(Refusal::new(Code::Failed, "~/code/web-shop already exists on vps.lan")),
        command => ready(command),
    }
}

fn rambling(command: &str) -> Structured {
    match command {
        "clone" => Structured::Error(Refusal::new(
            Code::Failed,
            "fatal: could not create\nwork tree dir because the disk on this machine is completely full",
        )),
        command => ready(command),
    }
}

fn cloned_with_flows(command: &str) -> Structured {
    match command {
        "clone" => cloned(command),
        command => start_flow::running(command),
    }
}

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the folder");
    std::fs::write(path, contents).expect("write the file");
}

fn not_there(told: fn(&str) -> Structured) -> Reading {
    let mut reading = reading_with(Reply::Told(told), Vec::new());
    let app = &mut reading.remote.rig.app;
    let root = app.projects[0].path.clone();
    git(&root, &["init", "--quiet"]);
    write(&root, ".gitignore", ".env\nlocal.toml\nnode_modules/\n");
    write(&root, ".env", "KEY=1");
    write(&root, "local.toml", "port = 1");
    write(&root, "node_modules/left-pad/index.js", "x");
    app.projects[0].repo.remotes = vec![("origin".into(), URL.into())];
    reading.pick("run on…", "vps");
    reading.remote.rig.until("the machine answers its accounts", |app| app.accounts_there("vps") != Checked::Waiting);
    reading
}

fn form(app: &App) -> &CloneForm {
    let Some(Overlay::CloneOnMachine(cloning)) = &app.overlay else { panic!("the clone dialog is not open") };
    cloning.form()
}

impl Reading {
    fn clone_dialog(&mut self) {
        self.press("start");
        self.remote.rig.until("the ignored files are listed", |app| !matches!(form(app).files, Files::Reading));
    }

    fn tick(&mut self, path: &str) {
        let app = &mut self.remote.rig.app;
        let at = form(app).lines().iter().position(|line| line.text.contains(path)).expect("the file is listed");
        let list = ui::clone_on_machine::list(AREA);
        click(app, Position::new(list.x + 1, list.y + u16::try_from(at).expect("a row")));
    }

    fn press_in_clone(&mut self, label: &str) {
        let app = &mut self.remote.rig.app;
        let at = BUTTONS.iter().position(|shown| *shown == label).expect("the button");
        click(app, ui::clone_on_machine::buttons(AREA, &BUTTONS)[at].as_position());
    }

    fn asked_for_the_clone(&self) -> Vec<Value> {
        let remote = &self.remote;
        wait_until("the clone is checked", || remote.asked().iter().any(|asked| asked["args"]["project"] == CLONED));
        let asked = remote.asked().into_iter().filter(|asked| asked["command"] == "clone");
        asked.map(|asked| asked["args"].clone()).collect()
    }

    fn packing(&mut self) -> AppEvent {
        self.tick(".env");
        self.press_in_clone("clone");
        self.remote.held(|event| matches!(event, AppEvent::Machine(MachineEvent::Packed { .. })))
    }

    fn on_its_way(&mut self) -> AppEvent {
        self.clone_dialog();
        self.press_in_clone("skip files");
        let answer = self.remote.held_relay();
        self.press_in_clone("back");
        answer
    }

    fn asked_again(&mut self) {
        self.pick("run on…", "vps");
        let remote = &self.remote;
        wait_until("the machine is asked again", || {
            remote.asked().iter().filter(|asked| asked["command"] == "account-list").count() == 2
        });
    }

    fn lists_the_clone(&mut self) {
        serve(&mut self.remote.theirs, 2, shop(vec![ProjectInfo { id: CLONED, ..web_shop() }]));
        self.remote.rig.until("the machine lists the clone", |app| {
            app.machines.held()[0]
                .seen
                .as_ref()
                .is_some_and(|seen| seen.projects.iter().any(|there| there.id == CLONED))
        });
    }

    fn cloned(&mut self) {
        self.remote.rig.until("the machine answers the clone", |app| match &app.overlay {
            Some(Overlay::CloneOnMachine(cloning)) => !cloning.form().cloning,
            _ => true,
        });
    }
}

#[test]
fn opens_from_start_with_the_url_and_every_ignored_file_unticked() {
    let mut reading = not_there(cloned);

    reading.clone_dialog();

    let app = &mut reading.remote.rig.app;
    assert_eq!((form(app).url.text().as_str(), form(app).destination().as_str()), (URL, "~/code/web-shop"));
    insta::assert_snapshot!(dialog(app, WIDE));
}

#[test]
fn sends_only_the_ticked_files_and_never_an_ignored_folder() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    reading.tick(".env");

    reading.press_in_clone("clone");
    reading.cloned();

    let sent = reading.sent("clone").expect("a clone");
    assert_eq!((&sent["url"], &sent["parent"]), (&json!(URL), &json!("~/code")));
    let files = sent["files"].as_array().expect("the files");
    let paths: Vec<&Value> = files.iter().map(|file| &file["path"]).collect();
    assert_eq!(paths, [&json!(".env")]);
    assert_eq!(files[0]["data"], json!(STANDARD.encode("KEY=1")));
}

#[test]
fn skips_the_files_when_asked() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    reading.tick(".env");

    reading.press_in_clone("skip files");
    reading.cloned();

    let sent = reading.sent("clone").expect("a clone");
    assert_eq!(sent.get("files"), None);
}

#[test]
fn goes_back_to_the_issue_on_the_machine_checks_only_the_clone_and_says_it_in_a_toast() {
    let mut reading = not_there(half_copied);
    reading.clone_dialog();

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    assert_eq!(browser(app).machine().map(|target| target.name.as_str()), Some("vps"));
    assert_eq!(toast(app), Some("cloned web-shop on vps · not copied: local.toml (it is tracked there)"));
    assert_eq!((browser(app).notice.as_deref(), error(app)), (None, None));
    reading.asked_for_the_clone();
    let asked = reading.remote.asked();
    assert_eq!(asked.iter().filter(|asked| asked["command"] == "account-list").count(), 1);
}

#[test]
fn goes_back_without_sending_anything() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    reading.tick(".env");

    reading.press_in_clone("back");
    reading.asked_again();

    assert!(browser_open(&reading.remote.rig.app));
    assert_eq!(reading.sent("clone"), None);
}

#[test]
fn sends_the_files_ticked_before_back_whatever_the_dialog_chose_since() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    let packed = reading.packing();
    reading.press_in_clone("back");
    reading.pick("run on…", "this");

    reading.deliver(packed);
    reading.remote.rig.until("the machine answers the clone", |app| toast(app).is_some());

    let sent = reading.sent("clone").expect("a clone");
    assert_eq!(sent["files"][0]["path"], json!(".env"));
    assert_eq!(toast(&reading.remote.rig.app), Some("cloned web-shop on vps"));
}

#[test]
fn asks_nothing_and_leaves_alone_a_dialog_that_chose_this_computer_while_it_cloned() {
    let mut reading = not_there(cloned);
    let answer = reading.on_its_way();
    reading.pick("run on…", "this");

    reading.deliver(answer);
    reading.asked_again();

    let remote = &reading.remote;
    assert!(remote.asked().iter().all(|asked| asked["args"]["project"] != CLONED));
    assert_eq!(toast(&reading.remote.rig.app), Some("cloned web-shop on vps"));
}

#[test]
fn says_it_is_cloning_where_start_was_and_starts_nothing_until_it_answers() {
    let mut reading = not_there(cloned);
    let answer = reading.on_its_way();
    assert_eq!(row(&reading.remote.rig.app, "start").as_deref(), Some("start   cloning on vps…"));

    reading.press("start");

    let app = &reading.remote.rig.app;
    assert!(browser_open(app), "{:?}", app.overlay);
    assert_eq!((error(app), browser(app).starting), (None, false));
    reading.deliver(answer);
    assert_eq!(row(&reading.remote.rig.app, "start").as_deref(), Some("start   not on vps · clone…"));
}

#[test]
fn starts_no_issue_on_a_clone_the_machine_lists_before_it_answers() {
    let mut reading = not_there(cloned);
    let _answer = reading.on_its_way();
    reading.lists_the_clone();
    reading.pick("account…", "home");
    assert!(browser(&reading.remote.rig.app).there().is_some_and(|there| there.cloned));

    reading.press("start");

    let app = &reading.remote.rig.app;
    assert_eq!((error(app), browser(app).starting), (None, false));
}

#[test]
fn starts_no_flow_on_a_clone_the_machine_lists_before_it_answers() {
    let mut reading = not_there(cloned_with_flows);
    let _answer = reading.on_its_way();
    reading.lists_the_clone();

    reading.flow_from_the_issue();

    let start = start_flow::start(&reading.remote.rig.app);
    assert_eq!(start.there().map(|there| there.place.as_str()), Some("cloning on vps…"));
    assert_eq!((start.notice.as_deref(), start.waiting), (None, false));
}

#[test]
fn makes_no_workspace_on_a_clone_the_machine_lists_before_it_answers() {
    let mut reading = not_there(cloned);
    let _answer = reading.on_its_way();
    reading.lists_the_clone();
    let app = &mut reading.remote.rig.app;
    app.overlay = None;
    app.ask_new_workspace(0);
    reading.new_on_vps();

    reading.create("login");

    let app = &reading.remote.rig.app;
    assert!(
        matches!(app.overlay, Some(Overlay::NewWorkspace { creating: false, error: None, .. })),
        "{:?}",
        app.overlay
    );
}

#[test]
fn names_the_machine_as_it_is_called_when_the_clone_answers() {
    let mut reading = not_there(cloned);
    let answer = reading.on_its_way();
    reading.remote.rig.edit(vec![machine("box", "vps.lan")], Duration::from_secs(3));

    reading.deliver(answer);

    assert_eq!(toast(&reading.remote.rig.app), Some("cloned web-shop on box"));
}

#[test]
fn leaves_alone_a_dialog_whose_machine_left_while_it_cloned() {
    let mut reading = not_there(half_copied);
    let answer = reading.on_its_way();
    reading.remote.rig.edit(Vec::new(), Duration::from_secs(3));
    reading.remote.rig.app.follow_issues_run_on();

    reading.deliver(answer);

    let app = &reading.remote.rig.app;
    assert!(browser(app).run_on.as_ref().is_some_and(|run_on| run_on.lost.is_some()));
    assert_eq!(browser(app).notice, None);
    assert_eq!(toast(app), Some("cloned web-shop on vps · not copied: local.toml (it is tracked there)"));
}

#[test]
fn says_briefly_why_a_ticked_file_could_not_be_read() {
    let mut reading = not_there(cloned);
    let root = reading.remote.rig.app.projects[0].path.clone();
    let long = "settings/for/the/staging/servers/of/the/web/shop/local.toml";
    write(&root, long, "port = 2");
    reading.clone_dialog();
    reading.tick("staging");
    std::fs::remove_file(root.join(long)).expect("remove the file");
    std::fs::create_dir(root.join(long)).expect("put a folder in its place");

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    let said = toast(app).and_then(|toast| toast.strip_prefix("the clone on vps failed: ")).expect("a failure");
    assert_eq!((said.chars().count(), said.ends_with('…')), (60, true), "{said}");
    assert_eq!(form(app).error, None);
    assert_eq!(reading.sent("clone"), None);
}

#[rstest]
#[case::io(Error::Io(std::io::Error::other("secret/.env is not a regular file")), "Other")]
#[case::git(Error::Git("fatal: not a git repository: /home/ana/web-shop".into()), "git")]
fn logs_a_failure_to_read_the_files_by_its_kind_alone(#[case] error: Error, #[case] logged: &str) {
    assert_eq!(failure(&error), logged);
}

#[rstest]
#[case::short(refused, "the clone on vps failed: ~/code/web-shop already exists on vps")]
#[case::rambling(rambling, "the clone on vps failed: fatal: could not create work tree dir because the disk on t…")]
fn says_a_failed_clone_in_a_toast_and_lets_it_be_tried_again(#[case] told: fn(&str) -> Structured, #[case] said: &str) {
    let mut reading = not_there(told);
    reading.clone_dialog();

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    assert_eq!((toast(app), form(app).error.as_deref()), (Some(said), None));
    assert!(!form(app).cloning);
}
