use super::*;
use cornercase::project::group;
use cornercase::state;

#[test]
fn sidebar_shows_the_search_and_the_title() {
    let app = Harness::start();
    let areas = areas();

    let search = app.row(areas.search.y);
    assert!(search.contains("search projects"), "search row: {search:?}");
    let title = app.row(areas.title.y);
    assert!(title.starts_with(" Projects"), "title row: {title:?}");
}

#[test]
fn the_project_name_stays_after_cd() {
    let mut app = Harness::start();
    let dir = temp_dir("cd");

    app.send(format!("cd {}; echo cd-\"\"done\r", dir.display()).as_bytes());

    app.wait_for("the shell moved", |s| s.contains("cd-done"));
    assert!(app.row(list().y).contains(&first_entry()), "entry row: {:?}", app.row(list().y));
}

#[test]
fn new_project_opens_the_typed_folder() {
    let mut app = Harness::start();
    let name = format!("ccnw-{}", std::process::id());
    let dir = temp_dir_named(&name);

    app.open_project(1, &dir);

    app.wait_for("the project opens", |s| s.contains(&entry(&name)) && !s.contains("cancel"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_folder_picker_works_with_the_mouse() {
    let mut app = Harness::start();
    let name = format!("ccpk-{}", std::process::id());
    let dir = temp_dir_named(&name);
    for folder in ["alpha", "beta"] {
        std::fs::create_dir_all(dir.join(folder)).expect("create folder");
    }
    app.open_project(1, &dir.join("alpha"));
    app.wait_for("the first project opens", |s| s.contains(&entry("alpha")));
    app.open_picker(2);
    let picker = ui::picker_area(AREA);
    app.wait_for("the picker starts next to it", |s| s.contains(&format!("{name}/")) && s.contains("beta"));

    app.click(ui::picker_item(picker, 3, 0, 2).as_position());
    app.wait_for("the picker goes into it", |s| s.contains(&format!("{name}/beta/")));
    app.click(ui::picker_buttons(picker, "open")[0].as_position());

    app.wait_for("the second project opens", |s| s.contains(&entry("beta")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn close_button_removes_a_project() {
    let mut app = Harness::start();
    let name = format!("cccl-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));

    app.click(ui::close_button(list(), 1, &plain(2), 0, SidebarRow::Project(0)).as_position());
    app.wait_for("it asks first", |s| s.contains("Close the project"));
    app.click(ui::form_buttons(ui::form_area(AREA), "close")[0].as_position());

    app.wait_for("only the second one is left", |s| s.contains(&entry(&name)) && !s.contains(&first_entry()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_an_open_folder_switches_to_it() {
    let mut app = Harness::start();
    let name = format!("ccsw-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));

    app.open_project(2, &temp());

    app.wait_for("project 1 is active again", |s| s.contains(&format!("▌{}", first_entry())) && !s.contains("cancel"));
    let third = app.row(ui::new_project_button(list(), 1, &plain(2)).y);
    assert!(third.contains(ui::NEW_BUTTON), "a third entry opened: {third:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clicking_an_entry_switches_project() {
    let mut app = Harness::start();
    let name = format!("ccen-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.send(b"echo in-the-first\r");
    app.wait_for("output in project 1", |s| s.contains("in-the-first"));
    app.open_project(1, &dir);
    app.wait_for("project 2 active and empty", |s| s.contains(&entry(&name)) && !s.contains("in-the-first"));

    app.click(Position::new(list().x + 2, list().y));

    app.wait_for("back to project 1", |s| s.contains("in-the-first"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_search_switches_project() {
    let mut app = Harness::start();
    let name = format!("ccse-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.send(b"echo in-the-first\r");
    app.wait_for("output in project 1", |s| s.contains("in-the-first"));
    app.open_project(1, &dir);
    app.wait_for("project 2 active and empty", |s| s.contains(&entry(&name)) && !s.contains("in-the-first"));

    app.click(areas().search.as_position());
    app.send(temp().file_name().and_then(|n| n.to_str()).expect("temp dir name").as_bytes());
    app.wait_for("the project is found", |s| s.contains("enter goes to"));
    app.send(b"\r");

    app.wait_for("back to project 1", |s| s.contains("in-the-first") && !s.contains("enter goes to"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_narrow_terminal_gets_a_menu_bar() {
    let narrow = ui::COMPACT_WIDTH - 10;
    let areas = ui::layout(Rect::new(0, 0, narrow, ROWS), state::Widths::default());
    let mut app = Harness::open(Session::new(), ROWS, narrow);
    app.wait_for("the bar names the project", |s| s.lines().nth(1).is_some_and(|l| l.contains("≡")));

    app.send(b"stty size\r");
    app.wait_for("the shell gets the whole width", |s| s.contains(&format!("{} {narrow}", ROWS - ui::COMPACT_PITCH)));

    app.click(areas.bar.as_position());
    app.wait_for("the menu lists the workspaces", |s| s.contains("‹ Projects") && s.contains(ui::NEW_BUTTON));
    app.click(areas.back.as_position());
    app.wait_for("back lists the projects", |s| {
        s.contains(ui::NEW_BUTTON) && !s.contains("‹ Projects") && s.contains("Quit")
    });
}

#[test]
fn dragging_a_project_reorders_the_sidebar_and_is_saved() {
    let mut app = Harness::start();
    let name = format!("ccdr-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));
    let (x, first, second) = (list().x + 4, list().y + 1, list().y + 2);

    app.send(format!("\x1b[<0;{x};{second}M\x1b[<32;{x};{first}M\x1b[<0;{x};{first}m").as_bytes());

    let above = |text: &str, a: &str, b: &str| matches!((text.find(a), text.find(b)), (Some(i), Some(j)) if i < j);
    app.wait_for("the new project is first", |s| above(s, &entry(&name), &first_entry()));
    let path = |dir: &std::path::Path| format!("\"path\": \"{}\"", dir.display());
    app.session.wait_for_saved("the order is saved", |saved| above(saved, &path(&dir), &path(&temp())));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_workspace_with_its_own_worktree_is_created_and_removed() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    let config = format!("{{\"worktrees_dir\": \"{}\"}}", worktrees.display());
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    let repo = std::env::temp_dir().join(format!("ccwt-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join(".gitignore"), ".env\n").expect("write .gitignore");
    std::fs::write(repo.join(".worktreeinclude"), ".env\n").expect("write .worktreeinclude");
    std::fs::write(repo.join(".env"), "TOKEN=1\n").expect("write .env");
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    let mut app = Harness::open(session, ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    app.open_project(1, &repo);
    app.wait_for("the repo opens as project 2", |s| s.contains(&entry(&repo_name)));

    let at = ui::new_workspace_button(workspaces_list(), 1, &tab_lines(&[1])).as_position();
    app.click(at);
    app.wait_for("the new menu opens", |s| s.contains("new workspace") && s.contains("start a flow…"));
    app.pick(at, &["new workspace", "start a flow…"], 0);
    app.wait_for("the form opens", |s| s.contains("with its own worktree"));
    app.send(b"e2e/login\r");

    app.wait_for("the worktree workspace opens", |s| s.contains("e2e/login") && !s.contains("cancel"));
    let list = workspaces_list();
    let y = (list.y..list.bottom()).find(|&y| app.row(y).contains("e2e/login")).expect("the workspace row");
    let label_row = Rect { y, height: 1, ..list };
    let checkout = worktrees.join(&repo_name).join("e2e-login");
    assert_eq!(std::fs::read_to_string(checkout.join(".env")).ok().as_deref(), Some("TOKEN=1\n"));

    app.click(ui::row_close_button(label_row, 1).as_position());
    app.wait_for("it asks first", |s| s.contains("Remove workspace"));
    app.click(ui::form_buttons(ui::form_area(AREA), "remove")[0].as_position());

    app.wait_for("git removes it and says so", |s| s.contains("removed e2e/login") && !s.contains("Remove workspace"));
    assert!(!app.row(y).contains("e2e/login") && !checkout.exists(), "the row or the checkout is still there");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn right_click_renames_a_project() {
    let mut app = Harness::start();
    let at = Position::new(list().x + 3, list().y);

    app.right_click(at);
    app.wait_for("the menu opens", |s| s.contains("rename project"));
    app.pick(at, &["rename project"], 0);
    app.wait_for("the form opens", |s| s.contains("leave it empty"));
    app.send(&[0x7f; 64]);
    app.send(b"my-api\r");

    app.wait_for("the entry shows the new name", |s| s.contains(&entry("my-api")) && !s.contains("leave it empty"));
    app.send(b"cd /\r");
    app.send(b"echo cd-\"\"done\r");
    app.wait_for("the shell moved", |s| s.contains("cd-done"));
    assert!(app.text().contains(&entry("my-api")), "the name changed after cd:\n{}", app.text());
}

#[test]
fn a_project_moves_into_a_new_group() {
    let mut app = Harness::start();
    let at = Position::new(list().x + 3, list().y);

    app.open_new_menu(1, 1);
    app.wait_for("the group form opens", |s| s.contains("New group") && s.contains("right-click a project"));
    app.send(b"work\r");
    app.wait_for("the group shows", |s| s.contains(&format!("  ▾ {} work", group::GROUP_STYLES[0].0)));
    app.right_click(at);
    app.wait_for("the menu opens", |s| s.contains("move to group"));
    app.pick(at, &["rename project", "move to group"], 1);
    let group = format!("{} work", group::GROUP_STYLES[0].0);
    app.wait_for("the groups are listed", |s| s.contains(&format!(" {group} ")) && !s.contains("move to group"));
    app.click(ui::menu_item(ui::menu_area(AREA, at, &[group.as_str()]), 0).as_position());

    app.wait_for("the project is inside", |s| s.contains(&format!("▌  {}", first_entry())));
    app.session
        .wait_for_saved("the group is saved", |saved| saved.contains("\"groups\"") && saved.contains("\"group\": 0"));
}
