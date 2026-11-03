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

    fn deliver(&mut self, event: AppEvent) {
        self.remote.rig.app.handle_event(event, AREA).expect("handle event");
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
fn goes_back_to_the_issue_on_the_machine_and_checks_only_the_clone() {
    let mut reading = not_there(half_copied);
    reading.clone_dialog();

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    assert_eq!(browser(app).machine().map(|target| target.name.as_str()), Some("vps"));
    assert_eq!(browser(app).notice.as_deref(), Some("cloned on vps · not copied: local.toml (it is tracked there)"));
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
    reading.pick("run on…", "vps");

    let remote = &reading.remote;
    wait_until("the machine is asked again", || {
        remote.asked().iter().filter(|asked| asked["command"] == "account-list").count() == 2
    });
    assert!(browser_open(&reading.remote.rig.app));
    assert_eq!(reading.sent("clone"), None);
}

#[test]
fn sends_the_files_read_after_it_went_back_and_says_so_there() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    let packed = reading.packing();
    reading.press_in_clone("back");

    reading.deliver(packed);
    reading.remote.rig.until("the machine answers the clone", |app| browser(app).notice.is_some());

    let sent = reading.sent("clone").expect("a clone");
    assert_eq!(sent["files"][0]["path"], json!(".env"));
    assert_eq!(browser(&reading.remote.rig.app).notice.as_deref(), Some("cloned on vps"));
}

#[test]
fn never_sends_files_read_for_a_dialog_that_chose_this_computer() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    let packed = reading.packing();
    reading.press_in_clone("back");
    reading.pick("run on…", "this");

    reading.deliver(packed);
    reading.pick("run on…", "vps");

    let remote = &reading.remote;
    wait_until("the machine is asked again", || {
        remote.asked().iter().filter(|asked| asked["command"] == "account-list").count() == 2
    });
    assert_eq!(reading.sent("clone"), None);
    assert_eq!(toast(&reading.remote.rig.app), Some("the clone on vps was not sent"));
}

#[test]
fn leaves_alone_a_dialog_that_chose_this_computer_while_it_cloned() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    reading.press_in_clone("skip files");
    let answer = reading.remote.held_relay();
    reading.press_in_clone("back");
    reading.pick("run on…", "this");

    reading.deliver(answer);

    let app = &reading.remote.rig.app;
    assert_eq!((browser(app).machine(), browser(app).notice.as_deref()), (None, None));
}

#[test]
fn checks_the_clone_even_once_the_dialog_sent_something_else() {
    let mut reading = not_there(half_copied);
    reading.clone_dialog();
    reading.press_in_clone("clone");
    let answer = reading.remote.held_relay();
    reading.press_in_clone("back");
    reading.remote.rig.app.dialog_run_on().expect("the run on row").awaiting = Some(u64::MAX);

    reading.deliver(answer);

    let app = &reading.remote.rig.app;
    assert_eq!(browser(app).notice.as_deref(), Some("cloned on vps · not copied: local.toml (it is tracked there)"));
    reading.asked_for_the_clone();
}

#[test]
fn offers_no_second_clone_while_one_is_on_its_way() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();
    reading.press_in_clone("skip files");
    let answer = reading.remote.held_relay();
    reading.press_in_clone("back");

    reading.press("start");

    assert!(browser_open(&reading.remote.rig.app), "{:?}", reading.remote.rig.app.overlay);
    assert_eq!(error(&reading.remote.rig.app), Some("still cloning on vps…"));
    reading.deliver(answer);
    assert_eq!(browser(&reading.remote.rig.app).notice.as_deref(), Some("cloned on vps"));
}

#[test]
fn reaches_the_start_flow_opened_from_the_issue_while_it_cloned() {
    let mut reading = not_there(half_copied);
    reading.clone_dialog();
    reading.press_in_clone("skip files");
    let answer = reading.remote.held_relay();
    reading.press_in_clone("back");
    reading.lists_the_clone();
    reading.press("start a flow");

    reading.deliver(answer);

    let Some(Overlay::StartFlow { start, .. }) = &reading.remote.rig.app.overlay else {
        panic!("Start flow is not open")
    };
    assert_eq!(start.notice.as_deref(), Some("cloned on vps · not copied: local.toml (it is tracked there)"));
}

#[test]
fn leaves_alone_a_dialog_whose_machine_left_while_it_cloned() {
    let mut reading = not_there(half_copied);
    reading.clone_dialog();
    reading.press_in_clone("skip files");
    let answer = reading.remote.held_relay();
    reading.press_in_clone("back");
    reading.remote.rig.edit(Vec::new(), Duration::from_secs(3));
    reading.remote.rig.app.follow_issues_run_on();

    reading.deliver(answer);

    let app = &reading.remote.rig.app;
    assert!(browser(app).run_on.as_ref().is_some_and(|run_on| run_on.lost.is_some()));
    assert_eq!(browser(app).notice, None);
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

    let said = form(&reading.remote.rig.app).error.clone().expect("an error");
    assert_eq!((said.chars().count(), said.ends_with('…')), (60, true), "{said}");
    assert_eq!(reading.sent("clone"), None);
}

#[rstest]
#[case::io(Error::Io(std::io::Error::other("secret/.env is not a regular file")), "Other")]
#[case::git(Error::Git("fatal: not a git repository: /home/ana/web-shop".into()), "git")]
fn logs_a_failure_to_read_the_files_by_its_kind_alone(#[case] error: Error, #[case] logged: &str) {
    assert_eq!(failure(&error), logged);
}

#[rstest]
#[case::short(refused, "~/code/web-shop already exists on vps")]
#[case::rambling(rambling, "fatal: could not create work tree dir because the disk on t…")]
fn keeps_a_refusal_red_in_the_clone_dialog(#[case] told: fn(&str) -> Structured, #[case] said: &str) {
    let mut reading = not_there(told);
    reading.clone_dialog();

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    assert_eq!(form(app).error.as_deref(), Some(said));
}
