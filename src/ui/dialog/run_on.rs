use ratatui::Frame;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::text::cells;
use crate::ui::View;

const LABEL: u16 = 8;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunOnRow {
    pub(crate) spans: Vec<Span<'static>>,
    pub(crate) choices: usize,
}

impl RunOnRow {
    pub(in crate::ui) fn at(&self, x: u16) -> Option<usize> {
        let mut end = usize::from(LABEL);
        if usize::from(x) < end {
            return None;
        }
        for (at, span) in self.spans.iter().take(self.choices).enumerate() {
            end += cells(&span.content);
            if usize::from(x) < end {
                return Some(at);
            }
        }
        None
    }

    pub(in crate::ui) fn draw(&self, f: &mut Frame, view: &View, r: ratatui::layout::Rect) {
        let label =
            Span::styled(format!("{:<width$}", "run on", width = usize::from(LABEL)), Style::default().fg(view.muted));
        let line = Line::from([vec![label], self.spans.clone()].concat());
        f.render_widget(Paragraph::new(line), r);
    }
}
