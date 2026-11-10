use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use rstest::rstest;

use super::*;

fn from_the_window(ev: Event) -> Step {
    Step::Event(ServerEvent::Message(1, ClientMessage::Event(ev)))
}

fn request(text: &str) -> Step {
    Step::Event(ServerEvent::Message(1, ClientMessage::Request(text.into())))
}

fn key() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
}

fn pointer() -> Event {
    Event::Mouse(MouseEvent { kind: MouseEventKind::Moved, column: 4, row: 2, modifiers: KeyModifiers::NONE })
}

#[rstest]
#[case::the_refresh(Step::Refresh, Some(Trace::Refresh))]
#[case::a_key(from_the_window(key()), Some(Trace::Flood))]
#[case::the_pointer(from_the_window(pointer()), Some(Trace::Flood))]
#[case::a_request(request(r#"{"command":"collapse","args":{}}"#), Some(Trace::Step))]
#[case::a_scroll_request(request(r#"{"command":"scroll","args":{}}"#), Some(Trace::Flood))]
#[case::a_machine_panel_line(Step::Event(ServerEvent::App(AppEvent::a_machine_panel_line())), Some(Trace::Flood))]
#[case::a_ratio_request(request(r#"{"command":"ratio","args":{}}"#), Some(Trace::Step))]
#[case::a_request_that_is_not_json(request("{"), Some(Trace::Step))]
#[case::a_client_leaving(Step::Event(ServerEvent::Gone(1)), Some(Trace::Flood))]
#[case::a_client_that_cannot_attach(Step::Event(ServerEvent::Incompatible(1)), Some(Trace::Flood))]
#[case::a_job_answer(Step::Event(ServerEvent::App(AppEvent::Exited(1, 0))), Some(Trace::Step))]
#[case::drawing(Step::Draw, None)]
fn floods_are_traced_apart_from_requests_and_job_answers(#[case] step: Step, #[case] expected: Option<Trace>) {
    assert_eq!(step.traced(), expected);
}
