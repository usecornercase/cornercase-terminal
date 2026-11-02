use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;
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
    reading.remote.rig.until("the machine answers its accounts", |app| browser(app).accounts.is_some());
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
fn goes_back_to_the_issue_on_the_machine_and_checks_it_again() {
    let mut reading = not_there(half_copied);
    reading.clone_dialog();

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    assert_eq!(browser(app).machine().map(|target| target.name.as_str()), Some("vps"));
    assert_eq!(browser(app).notice.as_deref(), Some("cloned on vps · not copied: local.toml (it is tracked there)"));
    let remote = &reading.remote;
    wait_until("the machine is checked again", || {
        let asked = remote.asked();
        let accounts = asked.iter().filter(|asked| asked["command"] == "account-list").count();
        accounts == 2 && asked.iter().any(|asked| asked["args"]["project"] == json!(CLONED))
    });
}

#[test]
fn goes_back_without_sending_anything() {
    let mut reading = not_there(cloned);
    reading.clone_dialog();

    reading.press_in_clone("back");

    assert!(browser_open(&reading.remote.rig.app));
    assert_eq!(reading.sent("clone"), None);
}

#[test]
fn keeps_a_refusal_red_in_the_clone_dialog() {
    let mut reading = not_there(refused);
    reading.clone_dialog();

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    assert_eq!(form(app).error.as_deref(), Some("~/code/web-shop already exists on vps"));
}
