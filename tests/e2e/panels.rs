use super::*;

#[test]
fn a_todo_typed_in_the_panel_is_saved_next_to_the_session() {
    let mut app = Harness::start();
    app.click(areas().todo_button.as_position());
    app.wait_for("the todo panel opens", |s| s.contains("+ New TODO"));
    let panel = ui::layout_with(AREA, ui::Widths::default(), true, ui::Sidebar::default()).changes;
    let view = ui::todo::View {
        items: Vec::new(),
        scroll: 0,
        adding: None,
        light: false,
        muted: ratatui::style::Color::DarkGray,
        drag: None,
    };
    let button = ui::todo::rows(panel, &view).button();

    app.click(button.as_position());
    app.send(b"buy milk\r");

    app.wait_for("the item shows", |s| s.contains("○  buy milk"));
    app.session.wait_for_file("todos.json", "the item is saved", |saved| saved.contains("buy milk"));
}

#[test]
fn the_changes_panel_shows_what_changed_in_the_repo() {
    let repo = std::env::temp_dir().join(format!("ccch-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join("notes.txt"), "one\ntwo\n").expect("write file");
    git(&repo, &["add", "notes.txt"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    std::fs::write(repo.join("notes.txt"), "one\nTWO\n").expect("edit file");
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
