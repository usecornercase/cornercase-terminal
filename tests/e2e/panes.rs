use super::*;

#[test]
fn shell_output_shows_in_the_pane() {
    let mut app = Harness::start();

    app.send(b"echo hello-$((20+22))\r");

    app.wait_for("output in the pane", |s| s.contains("hello-42"));
}

#[test]
fn the_wheel_scrolls_back_through_a_shells_output() {
    let mut app = Harness::start();
    app.send(b"i=1; while [ $i -le 200 ]; do echo row-$i; i=$((i+1)); done\r");
    app.wait_for("the output ends", |s| s.contains("row-200"));
    let (x, y) = (pane().x + 3, pane().y + 3);

    for _ in 0..80 {
        app.send(format!("\x1b[<64;{x};{y}M").as_bytes());
    }
    app.wait_for("the first rows show with the badge", |s| s.contains("row-1 ") && s.contains(" more "));

    app.send(b"x");
    app.wait_for("a key brings the last rows back", |s| s.contains("row-200") && !s.contains(" more "));
}

#[test]
fn exiting_the_last_shell_keeps_the_project() {
    let mut app = Harness::start();

    app.send(b"exit\r");
    app.wait_for("the workspace has no tab", |s| s.contains("no tab open") && s.contains(&first_entry()));
    app.click(workspace_row(&[0], WorkspaceRow::NewTab(0)));

    app.wait_for("a new tab opens", |s| !s.contains("no tab open"));
    app.send(b"echo in-the-\"\"new-tab\r");
    app.wait_for("the new tab runs a shell", |s| s.contains("in-the-new-tab"));
    assert!(app.is_running(), "client exited");
}

#[test]
fn tabs_keep_their_own_shells() {
    let mut app = Harness::start();
    app.send(b"echo in-the-first\r");
    app.wait_for("output in the first tab", |s| s.contains("in-the-first"));

    app.click(workspace_row(&[1], WorkspaceRow::NewTab(0)));
    app.wait_for("the second tab is active and empty", |s| !s.contains("in-the-first") && s.contains("$ "));
    app.click(workspace_row(&[2], WorkspaceRow::Tab(0, 0)));

    app.wait_for("back to the first tab", |s| s.contains("in-the-first"));
}

#[test]
fn dragging_a_border_resizes_the_shell_and_is_saved() {
    let mut app = Harness::start();
    let border = areas().projects_border;
    let (from, to, y) = (border.x + 1, border.x + 11, border.y + 2);

    app.send(format!("\x1b[<0;{from};{y}M\x1b[<32;{to};{y}M\x1b[<0;{to};{y}m").as_bytes());
    app.send(b"stty size\r");

    let pane = areas().pane.width - 10;
    app.wait_for("the shell sees the narrower pane", |s| s.contains(&format!("{ROWS} {pane}")));
    let widths = format!("\"projects\": {}", ui::SIDEBAR_WIDTH + 10);
    app.session.wait_for_saved("the widths are saved", |saved| saved.contains(&widths));
}

#[test]
fn dragging_over_text_copies_it_and_says_so() {
    let mut app = Harness::start();
    app.send(b"clear; echo copy-\"\"me\r");
    app.wait_for("the text shows", |s| s.lines().any(|l| l.trim_end().ends_with("copy-me")));
    let y = (0..ROWS).find(|&y| app.row(y).trim_end().ends_with("copy-me")).expect("the row of the text") + 1;
    let (from, to) = (pane().x + 1, pane().x + 7);

    app.send(format!("\x1b[<0;{from};{y}M\x1b[<32;{to};{y}M\x1b[<0;{to};{y}m").as_bytes());

    let osc52 = String::from_utf8(cornercase::host::clipboard::osc52("copy-me")).expect("utf-8");
    app.wait_for_raw("the text reaches the outer terminal's clipboard", |raw| raw.contains(&osc52));
    app.wait_for("a toast says it was copied", |s| s.contains("copied to clipboard"));
}

#[test]
fn a_program_inside_can_copy_to_the_clipboard() {
    let mut app = Harness::start();

    app.send(b"printf '\\033]52;c;Y29weS1tZQ==\\007'\r");

    let osc52 = String::from_utf8(cornercase::host::clipboard::osc52("copy-me")).expect("utf-8");
    app.wait_for_raw("the copy reaches the outer terminal's clipboard", |raw| raw.contains(&osc52));
    app.wait_for("a toast says it was copied", |s| s.contains("copied to clipboard"));
}

#[test]
fn a_pane_splits_from_its_menu_and_closes_on_exit() {
    let mut app = Harness::start();
    let at = Position::new(pane().x + 2, pane().y + 2);
    let items = ["split right", "split down", "send right-clicks to the pane", "close pane"];
    let mut layout = split::Node::Leaf(0);
    layout.split(0, Dir::Right, 1);
    let (right, divider) = (layout.panes(pane())[1].1, layout.dividers(pane())[0].line);

    app.right_click(at);
    app.wait_for("the pane menu opens", |s| s.contains("split right"));
    app.pick(at, &items, 0);
    app.wait_for("the divider shows", |s| screen_column(s, divider.x).iter().all(|c| *c == '│'));
    app.send(b"stty size\r");

    app.wait_for("the new shell has its half", |s| s.contains(&format!("{} {}", right.height, right.width)));
    app.session.wait_for_saved("the layout is saved", |saved| saved.contains("\"right\""));
    app.send(b"exit\r");
    app.wait_for("the divider goes", |s| screen_column(s, divider.x).iter().all(|c| *c != '│'));
    app.send(b"stty size\r");
    app.wait_for("the first shell has the whole pane again", |s| {
        s.contains(&format!("{} {}", pane().height, pane().width))
    });
}

fn screen_column(screen: &str, x: u16) -> Vec<char> {
    screen.lines().map(|l| l.chars().nth(usize::from(x)).unwrap_or(' ')).collect()
}
