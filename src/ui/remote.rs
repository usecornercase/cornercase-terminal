use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};

use super::{muted, overlay_block, surface_colour, truncate_right};
use crate::host_theme::HostTheme;

pub const TITLE: &str = "connection lost";
pub const QUIT: &str = " quit ";
const WIDTH: u16 = 60;
const HEIGHT: u16 = 8;

pub struct Notice<'a> {
    pub host: &'a str,
    pub retry_in: Option<u64>,
    pub error: Option<&'a str>,
    pub hovered: bool,
}

pub fn area(screen: Rect) -> Rect {
    let width = WIDTH.min(screen.width);
    let height = HEIGHT.min(screen.height);
    Rect { x: screen.x + (screen.width - width) / 2, y: screen.y + (screen.height - height) / 2, width, height }
}

pub fn quit(screen: Rect) -> Rect {
    let r = area(screen);
    let width = u16::try_from(QUIT.chars().count()).unwrap_or(u16::MAX).min(r.width.saturating_sub(4));
    Rect {
        x: (r.right().saturating_sub(width + 2)).max(r.x),
        y: r.bottom().saturating_sub(2).max(r.y),
        width,
        height: 1,
    }
}

pub fn hits_quit(screen: Rect, at: Position) -> bool {
    quit(screen).contains(at)
}

pub fn draw(buf: &mut Buffer, theme: &HostTheme, notice: &Notice) {
    let screen = buf.area;
    let r = area(screen);
    let grey = muted(theme);
    Clear.render(r, buf);
    let block = overlay_block(grey, TITLE);
    let inner = block.inner(r);
    block.render(r, buf);
    let width = usize::from(inner.width.saturating_sub(2));
    let status = match notice.retry_in {
        Some(seconds) => format!("trying again in {seconds} s"),
        None => "connecting…".into(),
    };
    let lines = vec![
        Line::default(),
        Line::from(Span::styled(
            truncate_right(&format!("reconnecting to {}…", notice.host), width),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(status, Style::default().fg(grey))),
        Line::from(Span::styled(truncate_right(notice.error.unwrap_or_default(), width), Style::default().fg(grey))),
        Line::default(),
        Line::from(Span::styled(
            truncate_right(
                &format!("its shells keep running on {}", notice.host),
                width.saturating_sub(QUIT.len() + 1),
            ),
            Style::default().fg(grey),
        )),
    ];
    let text = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
    Paragraph::new(lines).render(text, buf);
    let button = quit(screen);
    let surface = surface_colour(theme.is_light() == Some(true));
    let style = if notice.hovered {
        Style::default().fg(Color::Black).bg(Color::Gray)
    } else {
        Style::default().fg(Color::Gray).bg(surface)
    };
    Paragraph::new(Span::styled(QUIT, style)).render(button, buf);
}

#[cfg(test)]
mod tests {
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
}
