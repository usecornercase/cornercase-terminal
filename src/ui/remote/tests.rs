use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;

fn drawn(width: u16, height: u16, notice: &Notice) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal.draw(|f| draw(f.buffer_mut(), &HostTheme::default(), notice)).expect("draw");
    terminal
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

        insta::assert_snapshot!(drawn(70, 12, &notice).backend());
    }

    #[test]
    fn says_it_is_connecting_while_an_attempt_runs() {
        let notice = Notice { host: "devbox", retry_in: None, error: None, hovered: false };

        insta::assert_snapshot!(drawn(70, 12, &notice).backend());
    }

    #[test]
    fn quit_is_where_it_is_drawn() {
        let notice = Notice { host: "devbox", retry_in: Some(1), error: None, hovered: false };
        let terminal = drawn(70, 12, &notice);
        let r = quit(Rect::new(0, 0, 70, 12));

        let text: String = (r.x..r.right()).map(|x| terminal.backend().buffer()[(x, r.y)].symbol()).collect();

        assert_eq!(text, QUIT);
    }

    #[test]
    fn fits_a_tiny_screen() {
        let notice =
            Notice { host: "a-very-long-host-name.example.com", retry_in: Some(15), error: None, hovered: true };

        let terminal = drawn(12, 4, &notice);

        assert!(area(terminal.backend().buffer().area).width <= 12);
    }
}
