#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;
use crate::app::clone_on_machine::{packed, told};
use crate::control::CloneFile;
use crate::git::ignored::{self, IgnoredFile};
use crate::machines::clone_form::{BUTTONS, CloneForm, Files};
use crate::test_util::{git, git_repo, wait_until};

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

fn garbled(command: &str) -> Structured {
    match command {
        "clone" => Structured::Ok(json!({ "path": "/home/ana/code/web-shop", "unwritten": "local.toml" })),
        command => ready(command),
    }
}

fn empty(command: &str) -> Structured {
    match command {
        "clone" => Structured::Ok(json!({})),
        command => ready(command),
    }
}

fn nameless(command: &str) -> Structured {
    match command {
        "clone" => Structured::Ok(json!({ "path": "/home/ana/code/web-shop" })),
        command => ready(command),
    }
}

fn pathless(command: &str) -> Structured {
    match command {
        "clone" => Structured::Ok(json!({ "path": "", "project": CLONED })),
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

fn ignoring(told: fn(&str) -> Structured) -> Reading {
    let mut reading = reading_with(Reply::Told(told), Vec::new());
    let app = &mut reading.remote.rig.app;
    let root = app.projects[0].path.clone();
    git(&root, &["init", "--quiet"]);
    write(&root, ".gitignore", ".env\nlocal.toml\nnode_modules/\n");
    write(&root, ".env", "KEY=1");
    write(&root, "local.toml", "port = 1");
    write(&root, "node_modules/left-pad/index.js", "x");
    app.projects[0].repo.remotes = vec![("origin".into(), URL.into())];
    reading
}

fn not_there(told: fn(&str) -> Structured) -> Reading {
    let mut reading = ignoring(told);
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
        self.listed();
    }

    fn listed(&mut self) {
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
fn keeps_what_was_ticked_when_the_files_listed_for_an_earlier_opening_come_late() {
    let mut reading = not_there(cloned);
    reading.press("start");
    let earlier = reading.remote.held(|event| matches!(event, AppEvent::Machine(MachineEvent::Ignored { .. })));
    reading.press_in_clone("back");
    reading.clone_dialog();
    reading.tick(".env");

    reading.deliver(earlier);

    let ticked = form(&reading.remote.rig.app).ticked();
    assert_eq!(ticked.iter().map(|file| file.path.as_path()).collect::<Vec<_>>(), [Path::new(".env")]);
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
fn asks_new_workspace_for_no_accounts_once_it_cloned() {
    let mut reading = ignoring(cloned);
    let app = &mut reading.remote.rig.app;
    app.overlay = None;
    app.ask_new_workspace(0);
    reading.new_on_vps();
    reading.create("login");
    reading.listed();
    reading.press_in_clone("skip files");
    let answer = reading.remote.held_relay();

    reading.deliver(answer);

    let app = &reading.remote.rig.app;
    assert!(matches!(app.overlay, Some(Overlay::NewWorkspace { .. })), "{:?}", app.overlay);
    assert!(!app.machines.held()[0].checks.asking_accounts());
    assert_eq!(reading.sent("account-list"), None);
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

fn a_folder_read(root: &Path) -> Result<Vec<CloneFile>> {
    git(root, &["init", "--quiet"]);
    std::fs::create_dir_all(root.join("secret/.env")).expect("a folder");
    let file = IgnoredFile { path: PathBuf::from("secret/.env"), size: 5, mode: 0o644, too_big: false };
    packed(&ignored::list(root).expect("list").root, &[file])
}

fn a_moved_folder(root: &Path) -> Result<Vec<CloneFile>> {
    let top = root.join("shop");
    std::fs::create_dir(&top).expect("the project folder");
    git(&top, &["init", "--quiet"]);
    write(&top, ".gitignore", ".env\n");
    write(&top, ".env", "KEY=1");
    let listing = ignored::list(&top).expect("list");
    std::fs::rename(&top, root.join("moved")).expect("move the folder");
    std::fs::create_dir(&top).expect("another folder");
    packed(&listing.root, &listing.files)
}

fn outside_git(root: &Path) -> Result<Vec<CloneFile>> {
    ignored::toplevel(root).map(|_| Vec::new())
}

#[rstest]
#[case::io(a_folder_read, "Other", "secret/.env")]
#[case::moved(a_moved_folder, "Other", "the project folder changed since its files were listed")]
#[case::git(outside_git, "git", "git")]
fn logs_a_failure_to_read_the_files_by_its_kind_alone(
    #[case] reading: fn(&Path) -> Result<Vec<CloneFile>>,
    #[case] logged: &str,
    #[case] said: &str,
) {
    let root = TempDir::new();

    let (result, failed) = told(reading(root.path()));

    let shown = result.expect_err("a failure");
    assert!(shown.contains(said), "{shown}");
    assert_eq!(failed.as_deref(), Some(logged));
}

#[test]
fn refuses_files_that_grew_past_what_a_clone_takes_since_they_were_listed() {
    let repo = git_repo(&[(".gitignore", "*.bin\n")]);
    let size = usize::try_from(ignored::MAX_FILE_BYTES).expect("a small cap");
    let listed: Vec<IgnoredFile> = (0..9)
        .map(|n| {
            let path = format!("{n}.bin");
            std::fs::write(repo.path().join(&path), vec![0; size]).expect("write the file");
            IgnoredFile { path: PathBuf::from(path), size: 1, mode: 0o644, too_big: false }
        })
        .collect();

    let (result, failed) = told(packed(&ignored::list(repo.path()).expect("list").root, &listed));

    assert_eq!((result.err().as_deref(), failed.as_deref()), (Some("over the 8 MiB a clone takes"), Some("Other")));
}

#[test]
fn sends_the_permissions_a_file_has_when_it_is_packed() {
    let repo = git_repo(&[(".gitignore", ".env\n")]);
    let path = repo.path().join(".env");
    std::fs::write(&path, "KEY=1").expect("write the file");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let listing = ignored::list(repo.path()).expect("list");

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");

    let files = packed(&listing.root, &listing.files).expect("packed");
    assert_eq!(files.iter().map(|file| file.mode).collect::<Vec<_>>(), [0o600]);
}

#[rstest]
#[case::short(refused, "the clone on vps failed: ~/code/web-shop already exists on vps")]
#[case::rambling(rambling, "the clone on vps failed: fatal: could not create work tree dir because the disk on t…")]
#[case::unreadable(garbled, "the clone on vps failed: its answer could not be read")]
#[case::empty(empty, "the clone on vps failed: its answer could not be read")]
#[case::without_a_project(nameless, "the clone on vps failed: its answer could not be read")]
#[case::without_a_path(pathless, "the clone on vps failed: its answer could not be read")]
fn says_a_failed_clone_in_a_toast_and_lets_it_be_tried_again(#[case] told: fn(&str) -> Structured, #[case] said: &str) {
    let mut reading = not_there(told);
    reading.clone_dialog();

    reading.press_in_clone("clone");
    reading.cloned();

    let app = &reading.remote.rig.app;
    assert_eq!((toast(app), form(app).error.as_deref()), (Some(said), None));
    assert!(!form(app).cloning);
}
