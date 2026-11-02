use super::*;

const UNREACHABLE: &str = "vps unreachable: nothing was sent";

fn in_pane(app: &App, x: u16) -> Position {
    let pane = app.layout(AREA).pane;
    Position::new(pane.x + x, pane.y + 1)
}

fn ordered(streaming: &mut Streaming) -> Value {
    request(&mut reach(&streaming.seeing.ends))
}

fn split(streaming: &mut Streaming, mouse: &str) -> Position {
    say(&mut streaming.surface, &THERE.split(1, 1, mouse));
    streaming
        .seeing
        .rig
        .until("the split arrives", |app| app.remote_divider_at(in_pane(app, 9), app.layout(AREA).pane).is_some());
    in_pane(&streaming.seeing.rig.app, 9)
}

fn divider_grabbed() -> Streaming {
    let mut streaming = streaming();
    let line = split(&mut streaming, "none");
    mouse(&mut streaming.seeing.rig.app, MouseEventKind::Down(MouseButton::Left), line);
    streaming
}

fn ok(orders: &mut UnixStream, value: &Value) {
    say(orders, &json!({"ok": value}));
}

#[test]
fn a_click_on_another_pane_there_focuses_it_in_the_view() {
    let mut streaming = streaming();
    split(&mut streaming, "none");
    let at = in_pane(&streaming.seeing.rig.app, 13);

    click(&mut streaming.seeing.rig.app, at);

    let asked = ordered(&mut streaming);
    assert_eq!((&asked["command"], &asked["view"]), (&json!("focus"), &json!(VIEW)));
    assert_eq!(asked["args"]["item"], serde_json::to_value(Item::Pane(2)).expect("an item"));
}

#[test]
fn the_wheel_scrolls_the_history_of_the_pane_there() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    let at = in_pane(&streaming.seeing.rig.app, 3);

    mouse(&mut streaming.seeing.rig.app, MouseEventKind::ScrollUp, at);

    let asked = ordered(&mut streaming);
    assert_eq!(
        (&asked["command"], &asked["args"]["pane"], &asked["args"]["lines"]),
        (&json!("scroll"), &json!(1), &json!(-3))
    );
}

#[test]
fn with_the_link_down_a_click_sends_nothing_and_a_toast_says_so() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    fall(&mut streaming.seeing);
    let at = in_pane(&streaming.seeing.rig.app, 3);

    click(&mut streaming.seeing.rig.app, at);

    assert_eq!(toast(&streaming.seeing.rig.app), Some(UNREACHABLE));
}

#[test]
fn moving_over_a_fallen_tab_says_nothing() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    fall(&mut streaming.seeing);
    let at = in_pane(&streaming.seeing.rig.app, 3);

    mouse(&mut streaming.seeing.rig.app, MouseEventKind::Moved, at);

    assert_eq!(toast(&streaming.seeing.rig.app), None);
}

#[test]
fn a_right_click_on_another_pane_reading_the_mouse_focuses_it_then_goes_to_it_in_order() {
    let mut streaming = streaming();
    split(&mut streaming, "press_release");
    let at = in_pane(&streaming.seeing.rig.app, 13);

    mouse(&mut streaming.seeing.rig.app, MouseEventKind::Down(MouseButton::Right), at);

    let mut orders = reach(&streaming.seeing.ends);
    let focus = request(&mut orders);
    ok(&mut orders, &json!({"pane": 2}));
    let press = request(&mut orders);
    assert_eq!(
        (&focus["command"], &press["command"], &press["args"]["pane"]),
        (&json!("focus"), &json!("input"), &json!(2))
    );
}

#[test]
fn dragging_over_a_pane_there_selects_and_copies_its_text_here() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    let app = &mut streaming.seeing.rig.app;
    mouse(app, MouseEventKind::Down(MouseButton::Left), in_pane(app, 1));
    mouse(app, MouseEventKind::Drag(MouseButton::Left), in_pane(app, 3));
    let mut orders = reach(&streaming.seeing.ends);
    let dragged = request(&mut orders);
    ok(&mut orders, &json!({"pane": 1, "text": "el"}));

    let app = &mut streaming.seeing.rig.app;
    mouse(app, MouseEventKind::Up(MouseButton::Left), in_pane(app, 4));
    let released = request(&mut orders);
    ok(&mut orders, &json!({"pane": 1, "text": "ell"}));

    streaming.seeing.rig.until("the text arrives", |app| toast(app) == Some(COPIED));
    assert_eq!((&dragged["command"], &dragged["args"]["to"]), (&json!("selection"), &json!({"x": 3, "y": 1})));
    assert_eq!((&released["args"]["from"], &released["args"]["copy"]), (&json!({"x": 1, "y": 1}), &json!(false)));
    assert_eq!(streaming.seeing.rig.app.take_host_writes(), vec![clipboard::osc52("ell")]);
}

#[test]
fn dragging_a_divider_there_sends_its_ratio() {
    let mut streaming = divider_grabbed();
    let app = &mut streaming.seeing.rig.app;

    mouse(app, MouseEventKind::Drag(MouseButton::Left), in_pane(app, 6));

    let asked = ordered(&mut streaming);
    assert_eq!(
        (&asked["command"], &asked["args"]["tab"], &asked["args"]["path"]),
        (&json!("ratio"), &json!(13), &json!([]))
    );
    assert_eq!(asked["args"]["ratio"], json!(f64::from(6.0_f32 / 18.0)));
}

#[test]
fn a_double_click_on_a_divider_there_splits_it_in_half() {
    let mut streaming = streaming();
    let line = split(&mut streaming, "none");

    click(&mut streaming.seeing.rig.app, line);
    click(&mut streaming.seeing.rig.app, line);

    let asked = ordered(&mut streaming);
    assert_eq!((&asked["command"], &asked["args"]["ratio"]), (&json!("ratio"), &json!(0.5)));
}

#[test]
fn esc_ends_dragging_a_divider_there() {
    let mut streaming = divider_grabbed();
    let app = &mut streaming.seeing.rig.app;

    send_key(app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(app.screen.drag, None);
}

#[test]
fn hovering_a_divider_of_the_tab_here_shows_no_resize_pointer() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    let app = &mut streaming.seeing.rig.app;
    let pane = term(app, app.active).id;
    app.split_pane(pane, Dir::Right, AREA, false).expect("split").expect("a new pane");
    let area = app.layout(AREA).pane;
    let line = app.tab().map(|tab| tab.layout.dividers(area)[0].line).expect("a divider here");

    app.screen.hover = Some(Position::new(line.x, line.y));

    assert_eq!(app.pointer(AREA), pointer::Shape::Default);
}
