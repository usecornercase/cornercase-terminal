use super::routine_panel::opened;
use super::*;
use crate::agents::context::{Pointer, Said};
use crate::control::server_token;
use crate::queue::outcome::{Pull, Taken};
use crate::ui::Overlay as View;

const STARTED: i64 = 1_794_474_000;
const RESULT: i64 = 1_794_476_460;

fn shop(app: &App) -> Task {
    let place = Place {
        project: "/work/shop".into(),
        branch: Some("routine/review-the-open-pull-requests".into()),
        ..Place::default()
    };
    Task { label: "Review the open pull requests".into(), place, ..task(app, STARTED, None) }
}

fn gone(app: &mut App) -> u64 {
    let id = app.panels.queued.schedule(None, shop(app)).expect("entry");
    app.panels.queued.started_at(id, 999, STARTED);
    id
}

fn keep(app: &mut App, id: u64, text: &str, record: Option<Pointer>) {
    let pull = Pull {
        number: 482,
        title: "Fix the login".into(),
        state: "open".into(),
        checks: Some("passing".into()),
        ..Pull::default()
    };
    let taken = Taken {
        agent: Some("claude".into()),
        record,
        branch: Some("routine/review-the-open-pull-requests".into()),
        pull_request: Some(pull),
    };
    let said = Some(Said { text: text.into(), written: Some("2026-11-12T09:41:00.000Z".into()) });
    app.panels.queued.keep_result(id, taken, said, RESULT);
}

fn open_row(app: &mut App) {
    app.sync_queued();
    click(app, queue_row(0));
}

fn update(app: &App) -> ui::Update {
    let View::Update(update) = overlay_shown(app, AREA) else { panic!("an update dialog") };
    update
}

fn lines(app: &App) -> String {
    let text: Vec<String> = update(app)
        .notes
        .iter()
        .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
        .collect();
    text.join("\n")
}

fn back_on_the_queue_tab(app: &App) -> bool {
    matches!(&app.overlay, Some(Overlay::Issues(b)) if b.tabs.get(b.tab) == Some(&IssueTab::Queue))
}

#[test]
fn enter_on_an_entry_whose_pane_is_gone_opens_its_detail_and_esc_goes_back_to_the_queue_tab() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480.", None);

    open_row(&mut app);

    assert!(matches!(app.overlay, Some(Overlay::QueueEntry(_))));
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(back_on_the_queue_tab(&app));
}

#[test]
fn a_click_outside_and_done_go_back_too() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480.", None);
    open_row(&mut app);

    click(&mut app, Position::new(0, 0));
    assert!(back_on_the_queue_tab(&app));

    open_row(&mut app);
    let [_, done] = ui::update_buttons(AREA, "copy", "done");
    click(&mut app, done.as_position());
    assert!(back_on_the_queue_tab(&app));
}

#[test]
fn a_failed_run_opens_its_detail_and_a_failed_one_off_entry_its_form() {
    let (mut app, _rx, _dirs) = opened();
    let one_off = app.panels.queued.schedule(None, shop(&app)).expect("entry");
    app.panels.queued.set_state(one_off, QueueState::Failed { reason: "no room".into() });

    open_row(&mut app);

    assert!(matches!(app.overlay, Some(Overlay::Queue(_))));
}

fn long_text() -> String {
    (0..60).map(|n| format!("line {n:02}")).collect::<Vec<_>>().join("\n")
}

#[test]
fn the_wheel_and_the_arrows_scroll_a_long_result() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, &long_text(), None);
    open_row(&mut app);
    let inside = ui::update_notes(AREA).as_position();

    mouse(&mut app, MouseEventKind::ScrollDown, inside);
    let wheeled = update(&app).scroll;
    send_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    let arrowed = update(&app).scroll;
    for _ in 0..200 {
        send_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    let furthest = update(&app).scroll;

    assert!(wheeled > 0 && arrowed > wheeled, "{wheeled} {arrowed}");
    let most = update(&app).notes.len() - usize::from(ui::update_notes(AREA).height);
    assert_eq!(furthest, most);
}

fn transcript(dir: &Path) -> Pointer {
    let reply = serde_json::json!({
        "type": "assistant", "isSidechain": false, "timestamp": "2026-11-12T09:41:00.000Z",
        "message": {"id": "m1", "model": "claude-opus-5-5", "role": "assistant",
                    "content": [{"type": "text", "text": "Reviewed #480 and #482."}]},
    });
    let prompt =
        serde_json::json!({"type": "user", "origin": {"kind": "human"}, "message": {"content": "review them"}});
    let path = dir.join("record.jsonl");
    std::fs::write(&path, format!("{prompt}\n{reply}\n")).expect("transcript");
    Pointer::Claude(path)
}

#[test]
fn conversation_reads_the_record_and_result_comes_back() {
    let (mut app, rx, dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480 and #482.", Some(transcript(dirs[1].path())));
    open_row(&mut app);
    assert_eq!(update(&app).extra, Some("conversation"));

    let extra = ui::update_extra(AREA, "copy", "done", "conversation");
    click(&mut app, extra.as_position());
    assert!(matches!(update(&app).note, Some(ui::Note::Busy(_))));
    refreshing(&mut app, &rx, "the conversation shows", |app| update(app).note.is_none());

    let shown = lines(&app);
    assert!(shown.contains("prompt · 2026-11-12 09:41") || shown.contains("prompt"), "{shown}");
    assert!(shown.contains("review them") && shown.contains("Reviewed #480 and #482."), "{shown}");
    assert_eq!(update(&app).extra, Some("result"));
    click(&mut app, ui::update_extra(AREA, "copy", "done", "result").as_position());
    assert!(lines(&app).contains("agent     claude"), "{}", lines(&app));
}

#[test]
fn a_gone_record_offers_no_conversation_and_says_why() {
    let (mut app, _rx, dirs) = opened();
    let id = gone(&mut app);
    let record = Pointer::Claude(dirs[1].path().join("missing.jsonl"));
    keep(&mut app, id, "Reviewed #480.", Some(record));

    open_row(&mut app);

    assert_eq!(update(&app).extra, None);
    assert!(lines(&app).contains("no longer has this conversation's record"), "{}", lines(&app));
}

#[test]
fn copy_copies_the_result_and_keeps_the_detail_open() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480.", None);
    open_row(&mut app);

    let [copy, _] = ui::update_buttons(AREA, "copy", "done");
    click(&mut app, copy.as_position());

    assert_eq!(app.take_host_writes(), [clipboard::osc52("Reviewed #480.")]);
    assert!(matches!(app.overlay, Some(Overlay::QueueEntry(_))));
}

#[test]
fn the_detail_closes_when_its_entry_is_removed() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    open_row(&mut app);

    app.panels.queued.remove(id).expect("removed");
    app.close_gone_queue_detail();

    assert!(matches!(app.overlay, Some(Overlay::Issues(_))));
}

#[test]
fn the_hint_says_what_enter_does() {
    let (mut app, _rx, _dirs) = opened();
    gone(&mut app);
    app.sync_queued();

    let shown = format!("{}", render(&mut app, AREA).backend());

    assert!(shown.contains("enter shows its result"), "{shown}");
}

mod pictures {
    use super::*;

    fn pictured() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
        let (mut app, rx, dirs) = opened();
        app.set_queue_zone("UTC".into());
        (app, rx, dirs)
    }

    fn picture(app: &mut App) -> String {
        format!("{}", render(app, AREA).backend())
    }

    #[test]
    fn the_detail_of_a_removed_run() {
        let (mut app, _rx, _dirs) = pictured();
        let id = gone(&mut app);
        app.panels.queued.set_state(id, QueueState::Removed);
        keep(&mut app, id, "Reviewed #480 and #482. #482 needed a rebase; done.\nLeft: #479 waits on a reply.", None);
        open_row(&mut app);
        insta::assert_snapshot!(picture(&mut app));
    }

    #[test]
    fn the_detail_of_a_custom_agent_says_why_it_has_no_message() {
        let (mut app, _rx, _dirs) = pictured();
        let id = gone(&mut app);
        let taken = Taken { agent: Some("fake".into()), branch: Some("feat/x".into()), ..Taken::default() };
        app.panels.queued.keep_result(id, taken, None, RESULT);
        open_row(&mut app);
        insta::assert_snapshot!(picture(&mut app));
    }

    #[test]
    fn the_detail_of_a_failed_run() {
        let (mut app, _rx, _dirs) = pictured();
        let routine = {
            let repeat = queue::repeat::Repeat::parse("day 09:00", "UTC", STARTED).expect("repeat");
            let task = Task { repeat: Some(repeat), ..shop(&app) };
            app.panels.queued.schedule(None, task).expect("routine")
        };
        let run = app.panels.queued.run_now(routine, STARTED).expect("run");
        app.panels.queued.set_state(run, QueueState::Failed { reason: "the project folder is gone".into() });
        app.sync_queued();
        let row = app.panels.queued.queue().entries.iter().position(|entry| entry.id == run).expect("row");
        click(&mut app, queue_row(u16::try_from(row).expect("row")));
        insta::assert_snapshot!(picture(&mut app));
    }

    #[test]
    fn the_conversation_of_a_run() {
        let (mut app, rx, dirs) = pictured();
        let id = gone(&mut app);
        keep(&mut app, id, "Reviewed #480 and #482.", Some(transcript(dirs[1].path())));
        open_row(&mut app);
        click(&mut app, ui::update_extra(AREA, "copy", "done", "conversation").as_position());
        refreshing(&mut app, &rx, "the conversation shows", |app| update(app).note.is_none());
        insta::assert_snapshot!(picture(&mut app));
    }
}

#[test]
fn the_server_token_marks_an_entry_as_this_servers() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);

    let state = &app.panels.queued.queue().entry(id).expect("entry").state;

    assert!(matches!(state, QueueState::Started { server, .. } if server == server_token()));
}
