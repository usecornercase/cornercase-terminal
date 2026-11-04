use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;

fn drawn(width: u16, height: u16, shown: &Shown) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal.draw(|f| draw(f.buffer_mut(), &HostTheme::default(), shown)).expect("draw");
    terminal
}

fn text_at(terminal: &Terminal<TestBackend>, r: Rect) -> String {
    (r.x..r.right()).map(|x| terminal.backend().buffer()[(x, r.y)].symbol()).collect()
}

fn switch(step: Step<'static>) -> Shown<'static> {
    Shown::Switch(Switch { from: THIS_MACHINE, to: "devbox", step, hovered: None })
}

mod notice {
    use super::*;

    #[test]
    fn says_where_it_reconnects_and_when() {
        let notice = Notice {
            host: "devbox",
            retry_in: Some(4),
            error: Some("ssh: connect to host devbox port 22: Connection refused"),
            hovered: false,
        };

        insta::assert_snapshot!(drawn(70, 12, &Shown::Lost(notice)).backend());
    }

    #[test]
    fn says_it_is_connecting_while_an_attempt_runs() {
        let notice = Notice { host: "devbox", retry_in: None, error: None, hovered: false };

        insta::assert_snapshot!(drawn(70, 12, &Shown::Lost(notice)).backend());
    }

    #[test]
    fn quit_is_where_it_is_drawn() {
        let shown = Shown::Lost(Notice { host: "devbox", retry_in: Some(1), error: None, hovered: false });
        let terminal = drawn(70, 12, &shown);
        let [(button, r)] = buttons(Rect::new(0, 0, 70, 12), shown.buttons())[..] else { panic!("one button") };

        assert_eq!((button, text_at(&terminal, r)), (Button::Quit, QUIT.to_string()));
    }

    #[test]
    fn fits_a_tiny_screen() {
        let notice =
            Notice { host: "a-very-long-host-name.example.com", retry_in: Some(15), error: None, hovered: true };

        let terminal = drawn(12, 4, &Shown::Lost(notice));

        assert!(area(terminal.backend().buffer().area).width <= 12);
    }
}

mod switching {
    use super::*;

    #[test]
    fn asks_before_reaching_a_machine_for_the_first_time() {
        insta::assert_snapshot!(drawn(70, 12, &switch(Step::Asking)).backend());
    }

    #[test]
    fn says_where_it_goes_while_it_connects() {
        insta::assert_snapshot!(drawn(70, 12, &switch(Step::Connecting)).backend());
    }

    #[test]
    fn says_why_it_could_not_switch() {
        let failed = switch(Step::Failed("`devbox` runs cornercase 0.11.9 and this machine 0.12.0"));

        insta::assert_snapshot!(drawn(70, 12, &failed).backend());
    }

    #[test]
    fn a_long_reason_goes_on_to_the_next_line_between_words() {
        let reason = "cornercase is already running in this terminal; nesting it is not supported";
        let terminal = drawn(70, 12, &switch(Step::Failed(reason)));
        let inner = area(Rect::new(0, 0, 70, 12));
        let row = |y| text_at(&terminal, Rect { y, ..inner }).trim_matches(['│', ' ']).to_string();

        assert_eq!(
            [row(inner.y + 3), row(inner.y + 4)],
            ["cornercase is already running in this terminal; nesting", "it is not supported"]
        );
    }

    #[test]
    fn each_button_is_where_it_is_drawn() {
        let shown = switch(Step::Asking);
        let terminal = drawn(70, 12, &shown);

        let drawn: Vec<(Button, String)> = buttons(Rect::new(0, 0, 70, 12), shown.buttons())
            .into_iter()
            .map(|(button, r)| (button, text_at(&terminal, r)))
            .collect();

        assert_eq!(drawn, [(Button::Connect, CONNECT.to_string()), (Button::Back, BACK.to_string())]);
    }

    #[test]
    fn a_press_finds_the_button_under_it() {
        let screen = Rect::new(0, 0, 70, 12);
        let shown = switch(Step::Asking);
        let [(_, connect), (_, back)] = buttons(screen, shown.buttons())[..] else { panic!("two buttons") };

        let found = [connect, back].map(|r| hit(screen, shown.buttons(), Position::new(r.x, r.y)));

        assert_eq!(found, [Some(Button::Connect), Some(Button::Back)]);
    }

    #[test]
    fn a_press_between_the_buttons_finds_none() {
        let screen = Rect::new(0, 0, 70, 12);
        let shown = switch(Step::Asking);
        let [(_, connect), _] = buttons(screen, shown.buttons())[..] else { panic!("two buttons") };

        assert_eq!(hit(screen, shown.buttons(), Position::new(connect.right(), connect.y)), None);
    }

    #[test]
    fn the_hovered_button_is_lit() {
        let screen = Rect::new(0, 0, 70, 12);
        let shown = Shown::Switch(Switch {
            from: THIS_MACHINE,
            to: "devbox",
            step: Step::Connecting,
            hovered: Some(Button::Back),
        });
        let terminal = drawn(70, 12, &shown);
        let [(_, back)] = buttons(screen, shown.buttons())[..] else { panic!("one button") };

        assert_eq!(terminal.backend().buffer()[(back.x, back.y)].bg, Color::Gray);
    }
}
