use rstest::rstest;

use super::*;
use crate::agents::activity::Activity;
use crate::star::{self, Answer, FIRST_AFTER_DAYS, LATER_FOR, Saved};

const T0: i64 = 1_760_000_000;
const DAY: i64 = 86_400;

struct Star {
    app: App,
    _rx: Receiver<AppEvent>,
    dir: TempDir,
    now: Instant,
}

impl Star {
    fn with(activity: Activity, days: u32) -> Self {
        let (mut app, rx, _dirs) = app_with(1);
        let dir = TempDir::new();
        let term = app.term_mut().expect("a pane");
        term.agent.follow(Some(agents::Known::Claude.name()));
        term.agent.update(Some(activity), true, Instant::now());
        app.star.saved = Saved { days, ..Saved::default() };
        app.star.path = Some(dir.path().join("star.json"));
        let mut star = Self { app, _rx: rx, dir, now: Instant::now() };
        star.draw();
        star
    }

    fn due() -> Self {
        Self::with(Activity::Idle, FIRST_AFTER_DAYS)
    }

    fn draw(&mut self) {
        self.now = Instant::now();
        render(&mut self.app, AREA);
    }

    fn offer(&mut self, clock: i64) {
        self.draw();
        self.app.offer_star(self.now, clock);
        self.draw();
    }

    fn shows(&self) -> bool {
        self.app.star.card
    }

    fn saved(&self) -> Saved {
        star::load(&self.dir.path().join("star.json"))
    }

    fn press(&mut self, button: ui::StarButton) {
        let (_, rect) = ui::star_buttons(AREA).into_iter().find(|(b, _)| *b == button).expect("button");
        click(&mut self.app, rect.as_position());
    }
}

mod schedule {
    use super::*;

    #[test]
    fn a_fresh_app_never_shows_it() {
        let mut s = Star::with(Activity::Idle, 0);
        s.offer(T0);
        assert!(!s.shows());
    }

    #[test]
    fn before_fourteen_active_days_it_stays_away() {
        let mut s = Star::with(Activity::Idle, FIRST_AFTER_DAYS - 2);
        s.offer(T0);
        assert!(!s.shows());
    }

    #[test]
    fn at_fourteen_active_days_it_shows_and_remembers_when() {
        let mut s = Star::due();
        s.offer(T0);
        assert!(s.shows());
        assert_eq!(s.saved().shown, Some(T0));
    }

    #[test]
    fn a_day_counts_only_with_a_window_and_an_agent() {
        let mut s = Star::with(Activity::Idle, 0);
        s.app.star.window = None;
        s.app.offer_star(s.now, T0);
        assert_eq!(s.app.star.saved.days, 0);
        s.offer(T0);
        s.offer(T0 + 60);
        assert_eq!(s.app.star.saved.days, 1);
        assert_eq!(s.saved().days, 1);
    }

    #[test]
    fn a_shell_alone_does_not_count_a_day() {
        let (mut app, _rx, _dirs) = app_with(1);
        render(&mut app, AREA);
        app.offer_star(Instant::now(), T0);
        assert_eq!(app.star.saved.days, 0);
    }

    #[test]
    fn without_a_window_it_does_not_show() {
        let mut s = Star::due();
        s.app.star.window = None;
        s.app.offer_star(s.now, T0);
        assert!(!s.shows());
    }
}

mod getting_in_the_way {
    use super::*;

    #[rstest]
    #[case::working(Activity::Working)]
    #[case::waiting(Activity::Waiting)]
    #[case::failed(Activity::Failed)]
    fn a_busy_agent_keeps_it_away(#[case] activity: Activity) {
        let mut s = Star::with(activity, FIRST_AFTER_DAYS);
        s.offer(T0);
        assert!(!s.shows());
    }

    #[test]
    fn a_card_closed_before_it_was_drawn_does_not_use_the_ninety_days() {
        let mut s = Star::due();
        s.app.offer_star(s.now, T0);
        assert!(s.shows());
        s.app.toast = Some(Toast::new("hello", ui::ToastIcon::Check));
        s.app.offer_star(s.now, T0);
        assert!(!s.shows());
        s.draw();
        assert_eq!(s.app.star.saved.shown, None);
        assert_eq!(s.saved().shown, None);
    }

    #[test]
    fn an_agent_starting_work_closes_it_unanswered() {
        let mut s = Star::due();
        s.offer(T0);
        assert!(s.shows());
        s.app.term_mut().expect("a pane").agent.update(Some(Activity::Working), true, Instant::now());
        s.offer(T0 + 60);
        assert!(!s.shows());
        s.app.term_mut().expect("a pane").agent.update(Some(Activity::Idle), true, Instant::now());
        s.offer(T0 + DAY);
        assert!(!s.shows());
        assert!(!s.app.star.saved.due(T0 + DAY));
    }

    #[test]
    fn an_overlay_keeps_it_away() {
        let mut s = Star::due();
        click(&mut s.app, areas().search.as_position());
        assert!(s.app.overlay.is_some());
        s.offer(T0);
        assert!(!s.shows());
    }

    #[test]
    fn a_todo_field_keeps_it_away() {
        let mut s = Star::due();
        s.app.show_side(Some(Side::Todo));
        s.app.todo.field = Some(todo::Field::default());
        s.offer(T0);
        assert!(!s.shows());
    }

    #[test]
    fn a_toast_keeps_it_away() {
        let mut s = Star::due();
        s.app.toast = Some(Toast::new("hello", ui::ToastIcon::Check));
        s.offer(T0);
        assert!(!s.shows());
    }

    #[test]
    fn the_compact_layout_skips_it() {
        let mut s = Star::due();
        let narrow = Rect { width: 80, ..AREA };
        render(&mut s.app, narrow);
        s.app.offer_star(Instant::now(), T0);
        assert!(!s.shows());
    }

    #[test]
    fn turning_it_off_keeps_it_away_and_closes_a_card_that_shows() {
        let mut s = Star::due();
        s.app.config.star_prompt = false;
        s.offer(T0);
        assert!(!s.shows());
        s.app.config.star_prompt = true;
        s.offer(T0);
        assert!(s.shows());
        s.app.config.star_prompt = false;
        s.offer(T0);
        assert!(!s.shows());
    }
}

mod answering {
    use super::*;

    #[test]
    fn later_hides_it_for_ninety_days() {
        let mut s = Star::due();
        s.offer(T0);
        s.press(ui::StarButton::Later);
        assert!(!s.shows());
        assert_eq!(s.saved().answer, Some(Answer::Later));
        s.offer(T0 + LATER_FOR - DAY);
        assert!(!s.shows());
        s.offer(T0 + LATER_FOR);
        assert!(s.shows());
    }

    #[test]
    fn star_copies_the_link_and_never_asks_again() {
        let mut s = Star::due();
        s.offer(T0);
        s.press(ui::StarButton::Star);
        assert!(!s.shows());
        assert_eq!(s.app.take_host_writes(), [clipboard::osc52(star::URL)]);
        assert_eq!(s.saved().answer, Some(Answer::Starred));
        s.offer(T0 + 3650 * DAY);
        assert!(!s.shows());
    }

    #[test]
    fn never_stops_it_for_good() {
        let mut s = Star::due();
        s.offer(T0);
        s.press(ui::StarButton::Never);
        assert_eq!(s.saved().answer, Some(Answer::Never));
        s.offer(T0 + 3650 * DAY);
        assert!(!s.shows());
    }
}

mod the_pane_keeps_what_it_had {
    use super::*;

    #[test]
    fn keys_reach_the_pane_and_the_card_stays() {
        let mut s = Star::due();
        s.offer(T0);
        send_key(&mut s.app, KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(s.app.term().expect("a pane").input_at.is_some());
        assert!(s.shows());
    }

    #[test]
    fn a_press_on_the_card_does_not_reach_the_pane() {
        let mut s = Star::due();
        s.offer(T0);
        let area = ui::star_area(AREA);
        click(&mut s.app, Position::new(area.x + 2, area.y + 1));
        assert!(s.shows());
        assert!(s.app.overlay.is_none());
    }

    #[test]
    fn the_release_of_a_press_that_closed_the_card_does_not_reach_the_pane() {
        let mut s = Star::due();
        s.offer(T0);
        let (_, rect) = ui::star_buttons(AREA)[1];
        mouse(&mut s.app, MouseEventKind::Down(MouseButton::Left), rect.as_position());
        assert!(!s.shows());
        mouse(&mut s.app, MouseEventKind::Drag(MouseButton::Left), Position::new(5, 5));
        assert!(s.app.drag.is_none());
        mouse(&mut s.app, MouseEventKind::Up(MouseButton::Left), Position::new(5, 5));
        assert!(!s.app.star.swallowed);
        assert!(s.app.drag.is_none());
    }
}
