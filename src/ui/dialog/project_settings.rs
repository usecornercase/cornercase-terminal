use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};

use super::flows::{draw_buttons, draw_hint, draw_text, text_inside};
use super::issues::issue_buttons_in;
use super::{Note, form_inner};
use crate::project::settings::{Field, SAVE};
use crate::todo::editor::Editor;
use crate::ui::View;
use crate::ui::widgets::overlay_block;

const WIDTH: u16 = 80;
const HEIGHT: u16 = 20;

pub(crate) fn project_settings_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(WIDTH);
    let height = area.height.min(HEIGHT);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn rows(area: Rect) -> [Rect; 6] {
    let [setup, setup_field, _, teardown, teardown_field, hint, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(project_settings_area(area)));
    [setup, setup_field, teardown, teardown_field, hint, buttons]
}

pub(crate) fn project_settings_field(area: Rect, field: Field) -> Rect {
    let rows = rows(area);
    text_inside(match field {
        Field::Setup => rows[1],
        Field::Teardown => rows[3],
    })
}

pub(crate) fn project_settings_buttons(area: Rect, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(rows(area)[5], labels)
}

pub(crate) struct ProjectSettings {
    pub(crate) title: String,
    pub(crate) setup: Editor,
    pub(crate) teardown: Editor,
    pub(crate) focus: Field,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectSettingsHit {
    Field { field: Field, line: usize, col: usize },
    Button(usize),
}

pub(crate) fn project_settings_hit(area: Rect, view: &ProjectSettings, pos: Position) -> Option<ProjectSettingsHit> {
    if let Some(i) = project_settings_buttons(area, &view.buttons).iter().position(|r| r.contains(pos)) {
        return Some(ProjectSettingsHit::Button(i));
    }
    [Field::Setup, Field::Teardown].into_iter().find_map(|field| {
        let r = project_settings_field(area, field);
        r.contains(pos).then(|| ProjectSettingsHit::Field {
            field,
            line: usize::from(pos.y - r.y),
            col: usize::from(pos.x - r.x),
        })
    })
}

pub(in crate::ui) fn draw_project_settings(f: &mut Frame, view: &View, settings: &ProjectSettings) {
    let area = f.area();
    let r = project_settings_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &settings.title), r);
    let rows = rows(area);
    let dim = Style::default().fg(view.muted);
    let lit = |field: Field| if settings.focus == field { Style::default().fg(Color::Cyan) } else { dim };
    f.render_widget(Paragraph::new(Span::styled("setup", lit(Field::Setup))), rows[0]);
    f.render_widget(Paragraph::new(Span::styled("teardown", lit(Field::Teardown))), rows[2]);
    let setup = draw_text(f, view, rows[1], &settings.setup).filter(|_| settings.focus == Field::Setup);
    let teardown = draw_text(f, view, rows[3], &settings.teardown).filter(|_| settings.focus == Field::Teardown);
    draw_hint(f, view, settings.note.as_ref(), &settings.hint, rows[4]);
    draw_buttons(f, view, &project_settings_buttons(area, &settings.buttons), &settings.buttons, SAVE);
    if let Some(pos) = setup.or(teardown) {
        f.set_cursor_position(pos);
    }
}
