use rstest::rstest;

use super::*;
use crate::accounts;
use crate::agents::Known;
use crate::app::interaction::Drag;

const COMPACT: Rect = Rect { x: 0, y: 0, width: 60, height: 30 };

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    dir: TempDir,
}

fn with_an_account() -> Setup {
    let (app, rx) = app();
    let mut s = Setup { app, rx, dir: TempDir::new() };
    s.app.set_accounts_dir(&s.dir.path().join("accounts"));
    s.add("work");
    s
}

impl Setup {
    fn add(&mut self, name: &str) {
        accounts::add(&self.dir.path().join("accounts"), Known::Claude, name, None).expect("add the account");
        self.app.relist_accounts();
        self.app.resize(AREA);
    }

    fn remove(&mut self, name: &str) {
        accounts::remove(&self.dir.path().join("accounts"), Known::Claude, name).expect("remove the account");
        self.app.relist_accounts();
        self.app.resize(AREA);
    }

    fn leaves(&self, area: Rect) -> Vec<(u64, Rect)> {
        self.app.tab().expect("a tab").shown(self.app.layout(area).pane)
    }

    fn leaf(&self) -> Rect {
        self.leaves(AREA)[0].1
    }

    fn footer(&self) -> Position {
        let leaf = self.leaf();
        Position::new(leaf.x + 2, leaf.bottom() - 1)
    }

    fn split(&mut self, label: &str) {
        let leaf = self.leaf();
        right_click(&mut self.app, Position::new(leaf.x + 2, leaf.y + 2));
        pick(&mut self.app, label);
        self.app.resize(AREA);
    }

    fn sizes(&self) -> Vec<(u16, u16)> {
        let tab = self.app.tab().expect("a tab");
        self.leaves(AREA).iter().map(|(id, _)| tab.pane_by(*id).expect("a pane").size()).collect()
    }

    fn reads_the_mouse(&mut self) {
        type_line(&mut self.app, "printf '\\033[?1000h\\033[?1006h'; cat -v");
        pump_until(&mut self.app, &self.rx, "the program reads the mouse", |a| {
            a.term().is_some_and(|t| t.emulator.mouse_mode() != mouse::MouseMode::None)
        });
    }

    fn after_a_marker(&mut self) -> String {
        type_text(&mut self.app, "marker");
        shows(&mut self.app, &self.rx, "marker");
        screen(&mut self.app)
    }
}

fn rows_below(leaf: Rect) -> (u16, u16) {
    (leaf.height - 1, leaf.width)
}

mod the_pty {
    use super::*;

    #[rstest]
    #[case::wide(AREA)]
    #[case::compact(COMPACT)]
    fn of_a_single_pane_leaves_the_last_row_to_the_footer(#[case] area: Rect) {
        let mut s = with_an_account();
        s.app.resize(area);
        let leaf = s.leaves(area)[0].1;
        let term = s.app.term().expect("a pane");
        assert_eq!((term.size(), term.emulator.size().expect("a size")), (rows_below(leaf), rows_below(leaf)));
    }

    #[test]
    fn is_one_row_shorter_in_the_shell_too() {
        let mut s = with_an_account();
        let (rows, cols) = rows_below(s.leaf());
        type_line(&mut s.app, "stty size");
        shows(&mut s.app, &s.rx, &format!("{rows} {cols}"));
    }

    #[rstest]
    #[case::side_by_side("split right")]
    #[case::stacked("split down")]
    fn of_each_pane_of_a_split_leaves_its_last_row(#[case] label: &str) {
        let mut s = with_an_account();
        s.split(label);
        let leaves: Vec<(u16, u16)> = s.leaves(AREA).into_iter().map(|(_, leaf)| rows_below(leaf)).collect();
        assert_eq!((leaves.len(), s.sizes()), (2, leaves));
    }

    #[test]
    fn loses_the_row_with_the_first_account_and_gets_it_back_with_the_last() {
        let (app, rx) = app();
        let mut s = Setup { app, rx, dir: TempDir::new() };
        s.app.set_accounts_dir(&s.dir.path().join("accounts"));
        s.app.relist_accounts();
        s.app.resize(AREA);
        let whole = (s.leaf().height, s.leaf().width);
        assert_eq!(s.sizes(), [whole]);

        s.add("work");
        s.add("home");
        assert_eq!(s.sizes(), [rows_below(s.leaf())]);

        s.remove("work");
        assert_eq!(s.sizes(), [rows_below(s.leaf())]);
        s.remove("home");
        assert_eq!(s.sizes(), [whole]);
    }

    #[test]
    fn keeps_its_size_while_a_program_starts_and_exits() {
        let mut s = with_an_account();
        let before = s.sizes();
        running_sleep(&mut s.app);
        s.app.resize(AREA);
        assert_eq!(s.sizes(), before);
        send_key(&mut s.app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        wait_until("sleep ends", || s.app.term().and_then(|t| t.program(&s.app.config)).as_deref() != Some("sleep"));
        s.app.resize(AREA);
        assert_eq!(s.sizes(), before);
    }

    #[test]
    fn takes_the_whole_pane_with_the_switcher_off() {
        let mut s = with_an_account();
        s.app.config.account_switcher = false;
        s.app.resize(AREA);
        assert_eq!(s.sizes(), [(s.leaf().height, s.leaf().width)]);
    }

    #[test]
    fn of_a_new_tab_starts_one_row_short() {
        let mut s = with_an_account();
        s.app.add_tab(workspace_at(&s.app, 0, 0), AREA).expect("a new tab");
        assert_eq!(s.sizes(), [rows_below(s.leaf())]);
    }
}

mod drawn {
    use super::*;

    #[test]
    fn the_terminal_fills_the_pane_but_its_last_row() {
        let mut s = with_an_account();
        let leaf = s.leaf();
        let (rows, cols) = s.app.term().expect("a pane").size();
        let full: String = (0..rows).map(|_| "#".repeat(usize::from(cols))).collect::<Vec<_>>().join("\r\n");
        let id = s.app.term().expect("a pane").id;
        s.app.handle_event(AppEvent::Output(id, 0, format!("\x1b[H{full}").into_bytes().into()), AREA).expect("output");

        let (buffer, _) = render_frame(&mut s.app, AREA, &Sight::default());

        let row = |y: u16| (leaf.x..leaf.right()).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>();
        assert_eq!(row(leaf.bottom() - 2), "#".repeat(usize::from(leaf.width)));
        assert_eq!(row(leaf.bottom() - 1).trim(), "claude: System ▾");
    }

    #[test]
    fn the_scrollback_badge_sits_on_the_terminal_s_last_row() {
        let mut s = with_an_account();
        numbered(s.app.term_mut().expect("a pane"), 100);
        s.app.term_mut().expect("a pane").emulator.scroll(-3);

        let (buffer, _) = render_frame(&mut s.app, AREA, &Sight::default());

        let leaf = s.leaf();
        let row = |y: u16| (leaf.x..leaf.right()).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>();
        assert!(row(leaf.bottom() - 2).contains("↓ 3 more"), "{}", row(leaf.bottom() - 2));
        assert_eq!(row(leaf.bottom() - 1).trim(), "claude: System ▾");
        mouse(&mut s.app, MouseEventKind::Down(MouseButton::Left), Position::new(leaf.right() - 4, leaf.bottom() - 2));
        assert_eq!(s.app.term().expect("a pane").emulator.below(), 0);
    }
}

mod a_click_on_the_footer {
    use super::*;

    #[test]
    fn never_reaches_a_program_that_reads_the_mouse() {
        let mut s = with_an_account();
        s.reads_the_mouse();
        let at = s.footer();

        mouse(&mut s.app, MouseEventKind::Down(MouseButton::Middle), at);
        mouse(&mut s.app, MouseEventKind::Up(MouseButton::Middle), at);
        mouse(&mut s.app, MouseEventKind::ScrollUp, at);
        click(&mut s.app, at);

        assert!(!s.after_a_marker().contains("^[[<"));
    }

    #[test]
    fn leaves_the_terminal_s_last_row_to_the_program() {
        let mut s = with_an_account();
        s.reads_the_mouse();
        let (rows, leaf) = (s.app.term().expect("a pane").size().0, s.leaf());

        click(&mut s.app, Position::new(leaf.x, leaf.bottom() - 2));

        shows(&mut s.app, &s.rx, &format!("^[[<0;1;{rows}M"));
    }

    #[test]
    fn leaves_a_program_s_drag_its_release() {
        let mut s = with_an_account();
        s.reads_the_mouse();
        let (rows, leaf) = (s.app.term().expect("a pane").size().0, s.leaf());

        press(&mut s.app, Position::new(leaf.x, leaf.bottom() - 2));
        mouse(&mut s.app, MouseEventKind::Up(MouseButton::Left), Position::new(leaf.x, leaf.bottom() - 1));

        shows(&mut s.app, &s.rx, &format!("^[[<0;1;{rows}m"));
    }

    #[test]
    fn starts_no_selection() {
        let mut s = with_an_account();
        let (at, inside) = (s.footer(), Position::new(s.leaf().x + 5, s.leaf().y));
        press(&mut s.app, at);
        assert!(matches!(s.app.screen.drag, Some(Drag::Footer(_))));
        mouse(&mut s.app, MouseEventKind::Drag(MouseButton::Left), inside);
        mouse(&mut s.app, MouseEventKind::Up(MouseButton::Left), inside);
        assert!(s.app.screen.drag.is_none());
        assert_eq!(s.app.term_mut().expect("a pane").emulator.finish_selection().ok().flatten(), None);
    }

    #[test]
    fn does_not_focus_an_inactive_pane() {
        let mut s = with_an_account();
        s.split("split right");
        let (active, (left, leaf)) = (s.app.term().expect("a pane").id, s.leaves(AREA)[0]);
        assert_ne!(active, left);

        click(&mut s.app, Position::new(leaf.x + 2, leaf.bottom() - 1));

        assert_eq!(s.app.term().map(|t| t.id), Some(active));
    }

    #[test]
    fn with_the_right_button_opens_the_pane_s_menu_even_for_a_program_that_wants_it() {
        let mut s = with_an_account();
        s.app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
        let id = s.app.term().expect("a pane").id;
        s.app.tab_mut().expect("a tab").toggle_right_clicks(id);

        let at = s.footer();
        right_click(&mut s.app, at);

        assert_eq!(menu_labels(&s.app).last().map(String::as_str), Some(PaneAction::Close.label()));
    }
}
