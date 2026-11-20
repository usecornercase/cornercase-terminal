use super::*;
use crate::editor::Editor;
use crate::list::Combo;
use crate::queue::Place;
use crate::queue::form::{Choice, Choices, Control, Form, When};

fn queue_row(when: &str, title: &str, meta: &str, colour: Option<Color>) -> QueueRow {
    QueueRow {
        fold: None,
        run: false,
        when: when.into(),
        title: title.into(),
        meta: meta.into(),
        hidden: None,
        colour,
        dim: false,
    }
}

fn queue_view(rows: Vec<QueueRow>) -> View<'static> {
    let queue = QueueList {
        filter: String::new(),
        rows,
        selected: Some(0),
        scroll: 0,
        empty: "nothing queued: new schedules a prompt, and schedule on an issue queues it".into(),
        tail: None,
        hint: "enter edits it · start now runs it at once".into(),
        buttons: vec!["new", "start now", "remove", "clear started", "done"],
    };
    View { overlay: Some(Overlay::QueueList(Box::new(queue))), ..view(&["shop"]) }
}

fn queue_list() -> View<'static> {
    queue_view(vec![
        queue_row("2026-10-14 14:30", "#482 Fix the login form", "pending · claude · shop › issue-482", None),
        queue_row("2026-10-14 15:00", "Update the docs", "started · pane 7 · codex · shop", Some(Color::Green)),
        queue_row("2026-10-14 16:00", "Fix logout", "failed · codex is not installed · codex · shop", Some(Color::Red)),
        queue_row("2026-10-14 17:00", "Review the tests", "missed · claude · api", Some(Color::Yellow)),
    ])
}

fn tail_of<'a>(view: &'a mut View<'static>) -> &'a mut QueueList {
    let Some(Overlay::QueueList(queue)) = &mut view.overlay else { panic!("the queue modal") };
    queue
}

#[test]
fn the_queue_lists_every_entry_with_its_state_in_colour() {
    let t = render_sized(&queue_list(), 120, 24);
    insta::assert_snapshot!(t.backend());
    let lines = lines(&t);
    let failed = lines.iter().position(|l| l.contains("Fix logout")).expect("the failed row");
    let x = lines[failed].find("failed").expect("its state");
    let x = u16::try_from(lines[failed][..x].chars().count()).expect("column");
    let y = u16::try_from(failed).expect("row");
    assert_eq!(t.backend().buffer()[(x, y)].fg, Color::Red);
}

#[test]
fn the_queue_border_is_drawn_at_the_corners_of_its_area() {
    let (top_left, bottom_right) = corners_of_its_area(&queue_list());
    assert_eq!((top_left.as_str(), bottom_right.as_str()), ("╭", "╯"));
}

fn routines() -> Vec<QueueRow> {
    let run = |when: &str, status: &str, origin: &str, colour| QueueRow {
        run: true,
        ..queue_row(when, status, origin, colour)
    };
    vec![
        QueueRow {
            fold: Some(false),
            ..queue_row("2026-10-15 09:00", "Review the open pull requests", "↻ every day 09:00 · claude · shop", None)
        },
        run("2026-10-14 09:00", "started · pane 7", "claude · shop › routine/review", Some(Color::Green)),
        run("2026-10-13 09:00", "removed", "claude · shop", Some(Color::DarkGray)),
        run("2026-10-12 09:00", "kept · 2 uncommitted", "claude · shop", Some(Color::Yellow)),
        queue_row("2026-10-15 14:30", "#482 Fix the login form", "pending · claude · shop › issue-482", None),
        QueueRow {
            fold: Some(true),
            hidden: Some(" · 2 runs".into()),
            ..queue_row("2026-10-16 08:00", "Nightly build", "↻ every day 08:00 · codex · api", None)
        },
        QueueRow {
            dim: true,
            ..queue_row("paused", "Triage new issues", "↻ every 2h · claude · shop", Some(Color::DarkGray))
        },
    ]
}

#[test]
fn the_queue_shows_routines_with_their_runs_under_them_a_folded_one_and_a_paused_one() {
    let mut view = queue_view(routines());
    let queue = tail_of(&mut view);
    queue.hint = "enter edits it · start now runs it once more · ← → fold its runs".into();
    queue.buttons = vec!["new", "start now", "pause", "remove", "clear started", "done"];
    let t = render_sized(&view, 120, 24);
    insta::assert_snapshot!(t.backend());
    let lines = lines(&t);
    let folded = lines.iter().find(|l| l.contains("Nightly build")).expect("the folded routine");
    assert!(folded.contains("▸ ") && folded.contains(" · 2 runs"), "{folded}");
    let open = lines.iter().find(|l| l.contains("Review the open")).expect("the open routine");
    assert!(open.contains("▾ "), "{open}");
}

#[test]
fn a_click_on_the_mark_folds_and_elsewhere_on_the_row_opens() {
    let view = queue_view(routines());
    let Some(Overlay::QueueList(queue)) = &view.overlay else { panic!("the queue modal") };
    let screen = Rect::new(0, 0, 120, 24);
    let list = queue_list_rows(screen);
    let at = |x: u16, y: u16| queue_list_hit(screen, queue, Position::new(list.x + x, list.y + y));
    assert_eq!(at(1, 0), Some(crate::queue::dialog::Hit::Fold(0)));
    assert_eq!(at(10, 0), Some(crate::queue::dialog::Hit::Row(0)));
    assert_eq!(at(1, 1), Some(crate::queue::dialog::Hit::Row(1)));
    let done = *queue_list_buttons(screen, &queue.buttons).last().expect("done");
    assert_eq!(queue_list_hit(screen, queue, done.as_position()), Some(crate::queue::dialog::Hit::Button(4)));
}

fn form(prompt: &str, note: Option<&str>) -> Form {
    let places = vec![
        Choice { label: "shop · root".into(), place: Place::default(), worktree: true },
        Choice { label: "shop › main".into(), place: Place::default(), worktree: false },
    ];
    let choices =
        Choices { places, place: Some(0), kinds: vec!["auto".into(), "claude".into()], agent: 1, flows: Vec::new() };
    let mut form = Form::new(choices, "Europe/Madrid".into(), "2026-10-14");
    form.prompt = Editor::new(prompt);
    form.open_in.choose(1);
    form.branch = Editor::new("fix-login");
    form.mode = Combo::new(vec!["default".into(), "plan".into()], Some(1));
    form.focus = Control::When;
    form.error = note.map(str::to_string);
    form
}

fn lines(t: &Terminal<TestBackend>) -> Vec<String> {
    let area = t.backend().buffer().area;
    (area.y..area.bottom()).map(|y| row_text(t, Rect { y, height: 1, ..area })).collect()
}

fn shown(form: Form, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut view = view(&["shop"]);
    view.overlay = Some(Overlay::Queue(Box::new(form)));
    render_sized(&view, width, height)
}

#[test]
fn renders_the_schedule_form() {
    insta::assert_snapshot!(shown(form("Fix the login form", None), 100, 24).backend());
}

#[test]
fn renders_the_kinds_of_time_in_an_open_list() {
    let mut f = form("Fix the login form", None);
    f.when.open();
    insta::assert_snapshot!(shown(f, 100, 24).backend());
}

#[test]
fn a_date_shows_its_day_and_its_clock_and_a_tab_shows_no_branch() {
    let mut f = form("Fix the login form", None);
    f.when = When::On.combo();
    f.open_in.choose(0);
    let lines = lines(&shown(f, 100, 24));
    assert!(lines.iter().any(|l| l.contains("When      on ")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.contains("2026-10-14   09:00")), "{lines:#?}");
    assert!(!lines.iter().any(|l| l.contains("fix-login")), "{lines:#?}");
}

#[test]
fn the_open_list_counts_as_part_of_the_dialog() {
    let mut f = form("fix", None);
    f.place.open();
    let screen = Rect::new(0, 0, 100, 12);
    let list = queue_form::list(screen, &f).expect("an open list");
    assert_eq!(queue_form::area_with_list(screen, &f), queue_form::area(screen).union(list));
}

#[rstest]
#[case::two_rows_short(20)]
#[case::one_row_short(21)]
fn a_short_form_shows_its_error_above_the_buttons(#[case] height: u16) {
    let lines = lines(&shown(form("fix", Some("choose a time in the future")), 100, height));
    let note = lines.iter().position(|l| l.contains("choose a time in the future")).expect("the note shows");
    let buttons = lines.iter().position(|l| l.contains(" schedule ") && l.contains(" cancel ")).expect("the buttons");
    assert!(note < buttons, "{lines:#?}");
}

#[test]
fn a_tiny_form_keeps_its_rows_inside_its_border() {
    let lines = lines(&shown(form("fix", None), 100, 8));
    let bottom = lines.iter().rposition(|l| l.contains('╰')).expect("the bottom border");
    assert!(!lines[bottom].contains("Mode") && !lines[bottom].contains("Agent"), "{lines:#?}");
}

#[test]
fn a_prompt_with_line_breaks_shows_them() {
    let lines = lines(&shown(form("first line\nsecond line", None), 100, 24));
    let first = lines.iter().position(|l| l.contains("│ first line ")).expect("the first line");
    assert!(lines[first + 1].contains("│ second line "), "{lines:#?}");
}

#[test]
fn the_prompt_is_taller_in_a_tall_window() {
    let f = form("Fix the login form", None);
    let screen = Rect::new(0, 0, 100, 40);
    assert_eq!(queue_form::text(screen, &f, Control::Prompt).height, 8);
    assert_eq!(queue_form::area(screen).height, 28);
    let short = Rect::new(0, 0, 100, 24);
    let shift = queue_form::rect(screen, &f, Control::Agent).y - queue_form::area(screen).y;
    assert_eq!(shift, queue_form::rect(short, &f, Control::Agent).y - queue_form::area(short).y + 5);
    insta::assert_snapshot!(shown(f, 100, 40).backend());
}

#[test]
fn a_short_window_keeps_three_rows() {
    let f = form("Fix the login form", None);
    let screen = Rect::new(0, 0, 100, 24);
    assert_eq!(queue_form::text(screen, &f, Control::Prompt).height, 3);
    assert_eq!(queue_form::area(screen).height, 23);
}

#[test]
fn a_scrolled_prompt_shows_how_much_is_hidden() {
    let text: Vec<String> = (1..=12).map(|n| format!("line {n}")).collect();
    let mut f = form(&text.join("\n"), None);
    f.prompt.scroll_by(4, 60, 3);
    let lines = lines(&shown(f, 100, 24));
    let label = lines.iter().position(|l| l.contains("Prompt")).expect("the label");
    assert!(lines[label].contains(" ↑ 4 more "), "{lines:#?}");
    assert!(lines[label + 1].contains("│ line 5 "), "{lines:#?}");
    assert!(lines[label + 4].contains(" ↓ 5 more "), "{lines:#?}");
}

#[test]
fn the_focused_prompt_says_what_enter_does() {
    let mut f = form("fix", None);
    let hint = "enter breaks the line · tab moves on";
    assert!(!lines(&shown(f.clone(), 100, 24)).iter().any(|l| l.contains(hint)));
    f.focus = Control::Prompt;
    assert!(lines(&shown(f, 100, 24)).iter().any(|l| l.contains(hint)));
}

#[test]
fn a_workspace_says_it_opens_a_new_tab_without_offering_a_choice() {
    let mut f = form("fix", None);
    f.place.choose(1);
    let lines = lines(&shown(f, 100, 24));
    let open_in = lines.iter().find(|l| l.contains("Open in")).expect("the open in row");
    assert!(open_in.contains("Open in   new tab") && !open_in.contains('▾'), "{open_in}");
}

fn accounts(active: &str) -> crate::accounts::AgentAccounts {
    let labels = ["System", "home", "work"].map(String::from).into();
    crate::accounts::AgentAccounts::new("claude".into(), &["home".into(), "work".into()], labels, active)
}

#[test]
fn renders_the_account_beside_the_agent() {
    let mut f = form("Fix the login form", None);
    f.account = Some(accounts("work"));
    insta::assert_snapshot!(shown(f, 100, 24).backend());
}

#[test]
fn a_click_on_the_account_field_hits_it_and_its_open_list_chooses() {
    let mut f = form("fix", None);
    f.account = Some(accounts(""));
    let screen = Rect::new(0, 0, 100, 24);
    let field = queue_form::rect(screen, &f, Control::Account);
    assert!(matches!(
        queue_form::hit(screen, &f, field.as_position()),
        Some(queue_form::Hit::Control(Control::Account, 0, _))
    ));
    f.account.as_mut().expect("combo").combo.open();
    let list = queue_form::list(screen, &f).expect("the open list");
    let home = Position::new(list.x + 2, list.y + 2);
    assert!(matches!(queue_form::hit(screen, &f, home), Some(queue_form::Hit::Choose(1))));
}

fn reversed(t: &Terminal<TestBackend>) -> usize {
    t.backend().buffer().content().iter().filter(|c| c.modifier.contains(Modifier::REVERSED)).count()
}

#[test]
fn a_selection_in_the_focused_prompt_is_reversed() {
    let mut f = form("fix the login", None);
    f.focus = Control::Prompt;
    let before = reversed(&shown(f.clone(), 100, 24));
    f.prompt.select_all();
    assert_eq!(reversed(&shown(f, 100, 24)), before + 13);
}

#[test]
fn an_unfocused_field_draws_no_selection() {
    let mut f = form("fix the login", None);
    let before = reversed(&shown(f.clone(), 100, 24));
    f.prompt.select_all();
    assert_eq!(reversed(&shown(f, 100, 24)), before);
}

fn routine_form(when: When) -> Form {
    let mut f = form("Review the open pull requests", None);
    f.when = when.combo();
    f.flows = vec!["Quick".into()];
    f.refit();
    f
}

#[test]
fn a_routine_shows_its_repeat_the_checkbox_and_a_worktree_it_makes_itself() {
    let mut f = routine_form(When::Daily);
    f.cleanup = true;
    f.focus = Control::Cleanup;
    let t = shown(f, 100, 24);
    insta::assert_snapshot!(t.backend());
    let lines = lines(&t);
    assert!(lines.iter().any(|l| l.contains("When      every day at")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.contains("Open in   new worktree")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.contains("[x] remove the workspace when it finishes")), "{lines:#?}");
}

#[test]
fn the_checkbox_is_greyed_with_its_reason_when_the_agent_cannot_be_read() {
    let mut f = routine_form(When::Every);
    f.cleanup_off = Some("cornercase cannot tell when mine finishes".into());
    let t = shown(f, 100, 24);
    insta::assert_snapshot!(t.backend());
    let lines = lines(&t);
    assert!(lines.iter().any(|l| l.contains("at least 30 minutes apart")), "{lines:#?}");
    let at = lines.iter().position(|l| l.contains("[ ] remove the workspace")).expect("the checkbox row");
    let x = lines[at].find("[ ]").expect("its box");
    let x = u16::try_from(lines[at][..x].chars().count()).expect("column");
    let y = u16::try_from(at).expect("row");
    assert_eq!(t.backend().buffer()[(x, y)].fg, view(&["shop"]).muted);
}

#[test]
fn a_flow_replaces_the_mode_and_the_account() {
    let mut f = routine_form(When::Weekly);
    f.agent.choose(2);
    let t = shown(f, 100, 24);
    insta::assert_snapshot!(t.backend());
    let lines = lines(&t);
    assert!(lines.iter().any(|l| l.contains("set by the flow's roles")), "{lines:#?}");
}

#[test]
fn a_bot_hides_the_agent_mode_account_and_open_in_rows() {
    let places = vec![Choice {
        label: "bot: PRs".into(),
        place: Place { bot: Some("prs".into()), ..Place::default() },
        worktree: false,
    }];
    let choices =
        Choices { places, place: Some(0), kinds: vec!["auto".into(), "claude".into()], agent: 1, flows: Vec::new() };
    let mut bot = Form::new(choices, "Europe/Madrid".into(), "2026-10-14");
    bot.prompt = Editor::new("Review my pull requests");
    bot.when = When::Weekdays.combo();
    bot.focus = Control::When;
    let t = shown(bot, 100, 24);
    insta::assert_snapshot!(t.backend());
    let text = lines(&t).join("\n");
    assert!(text.contains("set by the bot") && text.contains("Each run is a new conversation"), "{text}");
    let at = lines(&t).iter().position(|l| l.contains("Where")).expect("the Where row");
    assert!(!lines(&t)[at].contains('▾'), "Where is fixed text: {text}");
    let x = lines(&t)[at].find("bot: PRs").expect("Where shows the bot");
    let x = u16::try_from(lines(&t)[at][..x].chars().count()).expect("column");
    assert_eq!(t.backend().buffer()[(x, u16::try_from(at).expect("row"))].fg, Color::DarkGray);
    for hidden in ["Mode", "Open in", "[ ]", "new tab", "Account"] {
        assert!(!text.contains(hidden), "{hidden} in\n{text}");
    }
}

fn with_tail(mut view: View<'static>, tail: &str, empty: bool) -> View<'static> {
    let queue = tail_of(&mut view);
    queue.tail = Some(tail.into());
    if empty {
        queue.rows.clear();
    }
    view
}

#[test]
fn the_queue_ends_with_a_dim_line_naming_the_bot_routines() {
    let t = render_sized(&with_tail(queue_list(), "2 bot routines: in each bot's chat", false), 120, 24);
    let lines = lines(&t);
    let last = lines.iter().position(|l| l.contains("Review the tests")).expect("the last row");
    assert!(lines[last + 1].contains("2 bot routines: in each bot's chat"), "{lines:#?}");
    let x = lines[last + 1].find("2 bot").expect("the tail");
    let x = u16::try_from(lines[last + 1][..x].chars().count()).expect("column");
    let cell = &t.backend().buffer()[(x, u16::try_from(last + 1).expect("row"))];
    assert_eq!(cell.fg, Color::DarkGray);
}

#[test]
fn the_bot_routines_line_goes_under_the_empty_text() {
    let t = render_sized(&with_tail(queue_list(), "1 bot routine: in its bot's chat", true), 120, 24);
    let lines = lines(&t);
    let empty = lines.iter().position(|l| l.contains("nothing queued")).expect("the empty text");
    assert!(lines[empty + 1].contains("1 bot routine: in its bot's chat"), "{lines:#?}");
}

#[test]
fn the_bot_routines_line_is_left_out_when_the_rows_fill_the_list() {
    let mut view = queue_list();
    tail_of(&mut view).rows.extend((0..40).map(|_| queue_row("2026-10-14 14:30", "Fix", "pending", None)));
    let t = render_sized(&with_tail(view, "9 bot routines: in each bot's chat", false), 120, 24);
    assert!(!lines(&t).join("\n").contains("bot routines"));
}
