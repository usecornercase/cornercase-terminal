#![cfg(unix)]

use rstest::rstest;

use super::*;
use crate::agents::activity::Activity;
use crate::star::{self, Answer, Saved};

const T0: i64 = 1_760_000_000;
const DAY: i64 = 86_400;

struct Star {
    app: App,
    rx: Receiver<AppEvent>,
    dir: TempDir,
    dirs: Vec<TempDir>,
    now: Instant,
}

impl Star {
    fn new() -> Self {
        let (mut app, rx, dirs) = app_with(1);
        let dir = TempDir::new();
        app.config.star_prompt = true;
        app.messages.star.with_saved(Saved::default(), dir.path().join("star.json"));
        let mut star = Self { app, rx, dir, dirs, now: Instant::now() };
        star.draw();
        star
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

    fn prompts(&mut self, n: usize) {
        self.app.count_star_prompts(n);
        self.offer(T0);
    }

    fn shows(&self) -> bool {
        matches!(self.app.overlay, Some(Overlay::Star))
    }

    fn saved(&self) -> Saved {
        star::load(&self.dir.path().join("star.json"))
    }

    fn press(&mut self, button: ui::StarButton) {
        let (_, rect) = ui::star_buttons(AREA).into_iter().find(|(b, _)| *b == button).expect("button");
        click(&mut self.app, rect.as_position());
    }

    fn restarted(&mut self) {
        let (mut app, rx, dirs) = app_with(1);
        app.config.star_prompt = true;
        app.set_star(self.dir.path().join("star.json"));
        self.app = app;
        self.rx = rx;
        self.dirs = dirs;
        self.draw();
    }
}

mod opening {
    use super::*;

    #[test]
    fn four_prompts_open_nothing_and_the_fifth_opens_it() {
        let mut s = Star::new();
        s.prompts(4);
        assert!(!s.shows());
        s.prompts(1);
        assert!(s.shows());
        assert_eq!(s.saved().prompts, 5);
    }

    #[test]
    fn without_a_window_it_waits_for_one() {
        let mut s = Star::new();
        s.app.messages.star.forget_window();
        s.app.count_star_prompts(5);
        s.app.offer_star(s.now, T0);
        assert!(!s.shows());
        s.offer(T0);
        assert!(s.shows());
    }

    #[test]
    fn a_dialog_open_at_the_fifth_prompt_makes_it_wait() {
        let mut s = Star::new();
        click(&mut s.app, areas().search.as_position());
        assert!(matches!(s.app.overlay, Some(Overlay::Search(_))));
        s.prompts(5);
        assert!(matches!(s.app.overlay, Some(Overlay::Search(_))));
        send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
        s.offer(T0);
        assert!(s.shows());
    }

    #[test]
    fn the_compact_menu_makes_it_wait() {
        let mut s = Star::new();
        let narrow = Rect { width: 60, ..AREA };
        render(&mut s.app, narrow);
        s.app.screen.nav = Some(ui::Nav::Projects);
        s.app.count_star_prompts(5);
        s.app.offer_star(Instant::now(), T0);
        assert!(!s.shows());
        s.app.screen.nav = None;
        render(&mut s.app, narrow);
        s.app.offer_star(Instant::now(), T0);
        assert!(s.shows());
    }

    #[test]
    fn a_todo_field_with_the_keys_makes_it_wait() {
        let mut s = Star::new();
        s.app.show_side(Some(Side::Todo));
        s.app.panels.todo.start_adding();
        s.prompts(5);
        assert!(!s.shows());
    }

    #[test]
    fn the_compact_layout_shows_it() {
        let mut s = Star::new();
        render(&mut s.app, Rect { width: 60, ..AREA });
        s.app.count_star_prompts(5);
        s.app.offer_star(Instant::now(), T0);
        assert!(s.shows());
    }
}

mod staying_open {
    use super::*;

    #[rstest]
    #[case::working(Activity::Working)]
    #[case::waiting(Activity::Waiting)]
    #[case::failed(Activity::Failed)]
    #[case::idle(Activity::Idle)]
    fn a_busy_or_finished_agent_does_not_close_it(#[case] activity: Activity) {
        let mut s = Star::new();
        s.prompts(5);
        let term = s.app.term_mut().expect("a pane");
        term.agent.follow(Some(agents::Known::Claude.name()));
        term.agent.update(Some(activity), true, Instant::now());
        s.offer(T0 + 60);
        assert!(s.shows());
    }

    #[test]
    fn a_toast_or_time_does_not_close_it() {
        let mut s = Star::new();
        s.prompts(5);
        s.app.messages.toast = Some(Toast::new("hello", ui::ToastIcon::Check));
        s.draw();
        s.app.offer_star(s.now + Duration::from_secs(DAY.unsigned_abs()), T0 + 365 * DAY);
        assert!(s.shows());
    }

    #[test]
    fn a_click_outside_does_nothing() {
        let mut s = Star::new();
        s.prompts(5);
        click(&mut s.app, areas().search.as_position());
        click(&mut s.app, Position::new(2, 2));
        assert!(s.shows());
        assert_eq!(s.saved().answer, None);
    }

    #[test]
    fn enter_and_typing_do_nothing_and_reach_no_pane() {
        let mut s = Star::new();
        s.prompts(5);
        send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
        send_key(&mut s.app, KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(s.shows());
        assert_eq!(s.saved().answer, None);
        assert!(s.app.term().expect("a pane").input_at.is_none());
    }

    #[test]
    fn turning_it_off_closes_it() {
        let mut s = Star::new();
        s.prompts(5);
        assert!(s.shows());
        s.app.config.star_prompt = false;
        s.offer(T0);
        assert!(!s.shows());
    }

    #[test]
    fn while_off_nothing_is_counted_or_written() {
        let mut s = Star::new();
        s.app.config.star_prompt = false;
        s.prompts(5);
        assert!(!s.shows());
        assert_eq!(s.app.messages.star.saved().prompts, 0);
        assert!(!s.dir.path().join("star.json").exists());
    }
}

mod answering {
    use super::*;

    #[test]
    fn star_copies_the_link_and_toasts_it() {
        let mut s = Star::new();
        s.prompts(5);
        s.press(ui::StarButton::Star);
        assert!(!s.shows());
        assert_eq!(s.app.take_host_writes(), [clipboard::osc52(star::URL)]);
        let toast = s.app.messages.toast.as_ref().expect("a toast");
        assert_eq!(toast.message, format!("copied {}", star::URL));
        assert_eq!(s.saved().answer, Some(Answer::Starred));
        s.prompts(5);
        assert!(!s.shows());
    }

    #[rstest]
    #[case::no_thanks(Some(Answer::Never))]
    #[case::esc(None)]
    fn no_thanks_and_esc_close_it_for_good(#[case] answer: Option<Answer>) {
        let mut s = Star::new();
        s.prompts(5);
        if answer.is_some() {
            s.press(ui::StarButton::NoThanks);
        } else {
            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
        }
        assert!(!s.shows());
        assert_eq!(s.saved().answer, answer);
        assert!(s.saved().shown.is_some());
        s.prompts(5);
        assert!(!s.shows());
    }

    #[test]
    fn once_drawn_it_never_shows_again_after_a_restart() {
        let mut s = Star::new();
        s.prompts(5);
        assert!(s.shows());
        s.restarted();
        s.prompts(5);
        assert!(!s.shows());
    }

    #[test]
    fn closed_before_a_frame_showed_it_it_comes_back() {
        let mut s = Star::new();
        s.app.count_star_prompts(5);
        s.app.offer_star(s.now, T0);
        assert!(s.shows());
        s.app.config.star_prompt = false;
        s.app.offer_star(s.now, T0);
        assert!(!s.shows());
        s.draw();
        assert_eq!(s.saved().shown, None);
        s.app.config.star_prompt = true;
        s.offer(T0);
        assert!(s.shows());
    }
}

mod an_older_star_json {
    use super::*;

    fn older(answer: &str) -> Star {
        let mut s = Star::new();
        let text = crate::test_util::fixture!("star/v1.json").replace(r#""later""#, answer);
        std::fs::write(s.dir.path().join("star.json"), text).expect("written");
        s.restarted();
        s
    }

    #[rstest]
    #[case::starred(r#""starred""#)]
    #[case::never(r#""never""#)]
    fn a_star_or_never_answer_from_the_card_never_shows_it(#[case] answer: &str) {
        let mut s = older(answer);
        s.prompts(9);
        assert!(!s.shows());
    }

    #[test]
    fn anything_else_from_the_card_counts_from_zero() {
        let mut s = older(r#""later""#);
        s.prompts(4);
        assert!(!s.shows());
        s.prompts(1);
        assert!(s.shows());
    }
}

mod looks {
    use super::*;

    fn dialog(area: Rect) -> String {
        let (mut app, _rx) = empty_app();
        app.config.star_prompt = true;
        render(&mut app, area);
        app.count_star_prompts(5);
        app.offer_star(Instant::now(), T0);
        format!("{}", render(&mut app, area).backend())
    }

    #[test]
    fn wide() {
        insta::assert_snapshot!(dialog(AREA));
    }

    #[test]
    fn compact() {
        insta::assert_snapshot!(dialog(Rect { width: 60, ..AREA }));
    }
}
