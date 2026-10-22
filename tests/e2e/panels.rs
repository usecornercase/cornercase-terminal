use super::*;
use cornercase::config;
use cornercase::state;

#[test]
fn a_todo_typed_in_the_panel_is_saved_next_to_the_session() {
    let mut app = Harness::start();
    app.click(areas().todo_button.as_position());
    app.wait_for("the todo panel opens", |s| s.contains("+ New TODO"));
    let panel = ui::layout_with(AREA, state::Widths::default(), true, config::Sidebar::default()).changes;
    let view = ui::todo::View {
        items: Vec::new(),
        scroll: 0,
        adding: None,
        light: false,
        muted: ratatui::style::Color::DarkGray,
        drag: None,
        compact: false,
    };
    let button = ui::todo::rows(panel, &view).button();

    app.click(button.as_position());
    app.send(b"buy milk\r");

    app.wait_for("the item shows", |s| s.contains("○  buy milk"));
    app.session.wait_for_file("todos.json", "the item is saved", |saved| saved.contains("buy milk"));
}

#[test]
fn ctrl_left_in_a_todo_moves_by_word() {
    let mut app = Harness::start();
    app.click(areas().todo_button.as_position());
    app.wait_for("the todo panel opens", |s| s.contains("+ New TODO"));
    let panel = ui::layout_with(AREA, state::Widths::default(), true, config::Sidebar::default()).changes;
    let view = ui::todo::View {
        items: Vec::new(),
        scroll: 0,
        adding: None,
        light: false,
        muted: ratatui::style::Color::DarkGray,
        drag: None,
        compact: false,
    };
    let button = ui::todo::rows(panel, &view).button();

    app.click(button.as_position());
    app.send(b"fix the login\x1b[1;5Dx\r");

    app.session.wait_for_file("todos.json", "the item is saved", |saved| saved.contains("fix the xlogin"));
}

fn edited_repo(prefix: &str) -> (PathBuf, String) {
    let repo = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join("notes.txt"), "one\ntwo\n").expect("write file");
    git(&repo, &["add", "notes.txt"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    std::fs::write(repo.join("notes.txt"), "one\nTWO\n").expect("edit file");
    (repo, repo_name)
}

#[test]
fn the_changes_panel_shows_what_changed_in_the_repo() {
    let (repo, repo_name) = edited_repo("ccch");
    let mut app = Harness::start();
    app.open_project(1, &repo);
    app.wait_for("the repo opens as project 2", |s| s.contains(&entry(&repo_name)));
    app.wait_for("the button counts the changed file", |s| s.contains("Changes 1"));

    app.click(ui::changes_button(areas().issues, "Changes 1").as_position());

    app.wait_for("the panel shows the diff", |s| {
        s.contains("Uncommitted") && s.contains("notes.txt") && s.contains("TWO")
    });
    let _ = std::fs::remove_dir_all(&repo);
}

fn opened_repo(prefix: &str) -> (PathBuf, Harness) {
    let (repo, repo_name) = edited_repo(prefix);
    let mut app = Harness::start();
    app.open_project(1, &repo);
    app.wait_for("the repo opens as project 2", |s| s.contains(&entry(&repo_name)));
    (repo, app)
}

#[test]
fn each_window_draws_the_icons_its_terminal_has() {
    let (repo, mut ghostty) = opened_repo("ccicons");
    let env = [("TERM", "xterm-256color")];
    let plain = Terminal {
        bin: std::path::Path::new(bin()),
        args: &[],
        size: (ROWS, COLS),
        env: &env,
        reply: UNKNOWN_TERMINAL,
    };
    let mut other = Harness::run_answering(Arc::clone(&ghostty.session), &plain);
    other.wait_for("the second window shows the sidebar", |s| s.contains("Projects"));
    ghostty.wait_for("the button counts the changed file", |s| s.contains("Changes 1"));

    ghostty.click(ui::changes_button(areas().issues, "Changes 1").as_position());

    ghostty.wait_for("the row has its type icon", |s| s.contains("M \u{e64e} notes.txt"));
    other.wait_for("the row has none", |s| s.contains("M notes.txt") && !s.contains('\u{e64e}'));
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn scripts_see_the_changes_of_a_workspace() {
    let (repo, app) = opened_repo("ccsc");

    let list = app.session.says(&["changes", "list", "--json"]);
    let summary: serde_json::Value = serde_json::from_str(&list).expect("a summary");
    let diff = app.session.says(&["changes", "file", "notes.txt"]);

    assert_eq!(summary["workspaces"][0]["files"][0]["path"], "notes.txt", "{list}");
    assert!(diff.contains("-two") && diff.contains("+TWO"), "{diff}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn scripts_discard_a_file_only_when_told_to() {
    let (repo, app) = opened_repo("ccdc");

    let asked = app.session.cli(&["changes", "discard", "notes.txt"]);
    assert_eq!(asked.status.code(), Some(1), "{asked:?}");
    assert!(String::from_utf8_lossy(&asked.stderr).contains("pass --yes"), "{asked:?}");
    assert_eq!(std::fs::read_to_string(repo.join("notes.txt")).expect("read"), "one\nTWO\n");
    let done = app.session.says(&["changes", "discard", "notes.txt", "--yes"]);

    assert_eq!(done, "discarded the changes to notes.txt");
    assert_eq!(std::fs::read_to_string(repo.join("notes.txt")).expect("read"), "one\ntwo\n");
    let _ = std::fs::remove_dir_all(&repo);
}

fn git_status(repo: &std::path::Path) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(repo).args(["status", "--porcelain"]).output();
    String::from_utf8_lossy(&out.expect("git").stdout).into_owned()
}

#[test]
fn scripts_stage_and_unstage_a_file() {
    let (repo, app) = opened_repo("ccst");

    let staged = app.session.says(&["changes", "stage", "notes.txt"]);
    assert_eq!(staged, "staged notes.txt");
    assert_eq!(git_status(&repo), "M  notes.txt\n");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let listed = app.session.says(&["changes", "list"]);
        if listed.contains("staged") {
            break;
        }
        assert!(Instant::now() < deadline, "the list never showed the file as staged: {listed}");
        thread::sleep(Duration::from_millis(100));
    }
    let unstaged = app.session.says(&["changes", "unstage", "notes.txt"]);

    assert_eq!(unstaged, "unstaged notes.txt");
    assert_eq!(git_status(&repo), " M notes.txt\n");
    let _ = std::fs::remove_dir_all(&repo);
}

fn showing_an_image(reply: &[u8], name: &str) -> Harness {
    let name = format!("{name}-{}", std::process::id());
    let dir = temp_dir_named(&name);
    let pixels = image::RgbaImage::from_pixel(400, 200, image::Rgba([200, 30, 30, 255]));
    pixels.save_with_format(dir.join("logo.png"), image::ImageFormat::Png).expect("write a png");
    let mut app = Harness::answering(reply);
    app.open_project(1, &dir);
    app.wait_for("the project opens", |s| s.contains(&entry(&name)) && !s.contains("cancel"));
    app.send(b"echo lo\"\"go.png\r");
    app.wait_for("the name is printed", |s| s.contains("logo.png"));
    let at = app.position_of("logo.png");
    app.click(at);
    app.wait_for("the image opens", |s| s.contains("400×200"));
    app
}

#[test]
fn ghostty_gets_placeholder_cells_and_the_image_itself() {
    let mut app = showing_an_image(GHOSTTY_1_3_1, "ccimg");

    app.wait_for_raw("the image is transmitted", |raw| raw.contains("\x1b_Ga=t,"));
    app.wait_for_raw("its placeholder cells are drawn", |raw| raw.contains('\u{10EEEE}'));
}

#[test]
fn an_unknown_terminal_is_told_why_it_shows_no_image() {
    let mut app = showing_an_image(UNKNOWN_TERMINAL, "ccnoimg");

    app.wait_for("the window says why", |s| s.contains("this terminal cannot show"));
    let raw = String::from_utf8_lossy(&app.raw.lock()).into_owned();
    assert!(!raw.contains("\x1b_G") && !raw.contains("1337;File"), "no image bytes reach it");
}
