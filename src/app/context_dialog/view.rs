use std::path::Path;

use ratatui::layout::Rect;
use ratatui::text::Line;

use super::REFRESH;
use crate::agents::Known;
use crate::agents::breakdown::{self, Breakdown, Category, Slot};
use crate::app::App;
use crate::app::pull_requests::DONE;
use crate::project::Hierarchy;
use crate::text::display_path;
use crate::ui::{self, ContextView, Fill, Row, Section, Slice};

const TOOLS_LISTED: usize = 5;

fn slices(breakdown: &Breakdown) -> (Vec<Slice>, Option<String>) {
    let slice = |category: &Category, fill: Fill| Slice {
        label: category.name.clone(),
        tokens: category.tokens,
        text: breakdown::tokens(category.tokens),
        share: breakdown::share(category.tokens, breakdown.max_tokens),
        fill,
    };
    let of = |slot: Slot| breakdown.categories.iter().filter(move |c| c.slot() == slot);
    let mut slices: Vec<Slice> = of(Slot::Used).enumerate().map(|(i, c)| slice(c, Fill::Used(i))).collect();
    slices.extend(of(Slot::Free).map(|c| slice(c, Fill::Free)));
    slices.extend(of(Slot::Buffer).map(|c| slice(c, Fill::Buffer)));
    let deferred: Vec<String> = of(Slot::Deferred)
        .map(|c| format!("{} {}", c.name.replace(" (deferred)", ""), breakdown::tokens(c.tokens)))
        .collect();
    (slices, (!deferred.is_empty()).then(|| format!("Not loaded: {}", deferred.join(" · "))))
}

fn row(left: String, middle: String, right: String) -> Row {
    Row { left, middle, right, muted: false, path: false }
}

fn memory(breakdown: &Breakdown, home: Option<&Path>) -> Option<Section> {
    let rows: Vec<Row> = breakdown
        .memory_files
        .iter()
        .map(|file| Row {
            path: true,
            ..row(display_path(Path::new(&file.path), home), String::new(), breakdown::tokens(file.tokens))
        })
        .collect();
    (!rows.is_empty()).then_some(Section { title: "Memory files", rows })
}

fn mcp(breakdown: &Breakdown) -> Vec<Section> {
    let tools: Vec<Row> = breakdown
        .mcp_servers
        .iter()
        .map(|server| Row {
            muted: server.loaded == 0,
            ..row(
                server.name.clone(),
                format!("{} of {} loaded", server.loaded, server.tools),
                breakdown::tokens(server.tokens),
            )
        })
        .collect();
    let instructions: Vec<Row> = breakdown
        .mcp_instructions
        .iter()
        .map(|item| row(item.name.clone(), String::new(), breakdown::tokens(item.tokens)))
        .collect();
    [("MCP tools", tools), ("MCP server instructions", instructions)]
        .into_iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(title, rows)| Section { title, rows })
        .collect()
}

fn skills(breakdown: &Breakdown) -> Option<Section> {
    let skills = breakdown.skills.as_ref().filter(|s| s.total > 0)?;
    let count = if skills.included == skills.total {
        skills.total.to_string()
    } else {
        format!("{} of {}", skills.included, skills.total)
    };
    Some(Section { title: "Skills", rows: vec![row(count, String::new(), breakdown::tokens(skills.tokens))] })
}

fn messages(breakdown: &Breakdown) -> Option<Section> {
    let found = breakdown.messages.as_ref()?;
    let muted = |left: String| Row { muted: true, ..row(left, String::new(), String::new()) };
    let mut rows = vec![row(
        "Tool calls and results".into(),
        String::new(),
        breakdown::tokens(found.tool_calls + found.tool_results),
    )];
    rows.push(muted(format!(
        "Assistant {} · You {} · Attachments {}",
        breakdown::tokens(found.assistant),
        breakdown::tokens(found.user),
        breakdown::tokens(found.attachments)
    )));
    let mut tools: Vec<_> = found.tools.iter().map(|t| (&t.name, t.calls + t.results)).collect();
    tools.sort_by_key(|(_, tokens)| std::cmp::Reverse(*tokens));
    let top: Vec<String> = tools
        .iter()
        .take(TOOLS_LISTED)
        .map(|(name, tokens)| format!("{name} {}", breakdown::tokens(*tokens)))
        .collect();
    if !top.is_empty() {
        rows.push(muted(top.join(" · ")));
    }
    Some(Section { title: "Messages", rows })
}

pub(super) fn breakdown_view(breakdown: &Breakdown, home: Option<&Path>) -> ContextView {
    let (slices, not_loaded) = slices(breakdown);
    let sections = memory(breakdown, home)
        .into_iter()
        .chain(mcp(breakdown))
        .chain(skills(breakdown))
        .chain(messages(breakdown))
        .collect();
    let notice = breakdown
        .conversation
        .is_none()
        .then(|| "No conversation yet: this is what a new conversation starts with.".to_string());
    ContextView { max: breakdown.max_tokens, slices, not_loaded, sections, notice }
}

impl App {
    pub(in crate::app) fn context_notes(&self, pane: u64, width: usize) -> Vec<Line<'static>> {
        let Some(term) = self.projects.pane_by(pane) else { return Vec::new() };
        let read = self.agents.contexts.reads.get(&pane);
        let view = match (read.and_then(|r| r.probe.last()), term.context.kind()) {
            (Some((breakdown, _)), _) => breakdown_view(breakdown, self.home.as_deref()),
            (None, Some(kind)) if kind.breakdown().is_none() => ContextView {
                notice: Some(format!("{} does not say what fills its context", kind.title())),
                ..ContextView::default()
            },
            _ => ContextView::default(),
        };
        ui::context_notes(&view, width, self.muted_color())
    }

    fn context_message(&self, pane: u64, kind: Known) -> String {
        let term = self.projects.pane_by(pane);
        let shown = term.and_then(|t| t.context.context());
        let head = shown.map_or_else(|| kind.title().to_string(), |c| format!("{} · {}", kind.title(), c.model));
        let read = self.agents.contexts.reads.get(&pane).and_then(|r| r.probe.last());
        let second = match (read, shown.and_then(|c| c.percent)) {
            (Some((b, _)), _) => {
                format!(
                    "{} of {} tokens · {}%",
                    breakdown::tokens(b.total_tokens),
                    breakdown::tokens(b.max_tokens),
                    b.percent
                )
            }
            (None, Some(percent)) => format!("{percent}% of its window used"),
            (None, None) if term.is_some_and(|t| t.context.compaction().hides) => "compacted".into(),
            (None, None) => String::new(),
        };
        if second.is_empty() { head } else { format!("{head}\n{second}") }
    }

    pub(in crate::app) fn context_view(&self, pane: u64, scroll: usize, area: Rect) -> Option<ui::Overlay> {
        let kind = self.projects.pane_by(pane)?.context.kind()?;
        let width = usize::from(ui::update_notes(area).width);
        let read = self.agents.contexts.reads.get(&pane);
        let note = match read {
            Some(read) if read.probe.loading() => Some(ui::Note::Busy("reading…")),
            Some(read) if read.probe.error().is_some() => {
                read.probe.error().map(|e| ui::Note::Error(format!("could not read the context: {e}")))
            }
            Some(read) => read.probe.last().map(|(_, at)| ui::Note::Info(breakdown::read_ago(at.elapsed().as_secs()))),
            None => None,
        };
        Some(ui::Overlay::Update(ui::Update {
            title: "Context",
            message: self.context_message(pane, kind),
            notes: self.context_notes(pane, width),
            scroll,
            note,
            submit: REFRESH,
            extra: None,
            cancel: DONE,
        }))
    }
}
