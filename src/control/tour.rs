use serde::{Deserialize, Serialize};

use super::Fields;
use crate::tour::{self, Done, Offer, Saved, TOURS, Target};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TourAction {
    #[default]
    List,
    Start,
    Next,
    Back,
    Skip,
    End,
    Complete,
    Reset,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TourRequest {
    pub(crate) action: TourAction,
    pub(crate) tour: Option<String>,
    pub(crate) step: Option<String>,
    pub(crate) card: Option<bool>,
}

impl TourRequest {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("action", format!("{:?}", self.action).to_lowercase());
        fields.maybe("tour", self.tour.as_deref());
        fields.maybe("step", self.step.as_deref());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ActiveInfo {
    pub(crate) tour: String,
    pub(crate) step: String,
    pub(crate) index: usize,
    pub(crate) card: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProgressInfo {
    pub(crate) tour: String,
    pub(crate) done: Vec<String>,
    pub(crate) finished: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TourState {
    pub(crate) active: Option<ActiveInfo>,
    pub(crate) offered: bool,
    pub(crate) progress: Vec<ProgressInfo>,
}

impl TourState {
    pub(crate) fn of(saved: &Saved, active: Option<ActiveInfo>) -> Self {
        let progress = TOURS
            .iter()
            .map(|t| {
                let p = saved.progress(t.id);
                ProgressInfo { tour: t.id.into(), done: p.done, finished: p.finished }
            })
            .collect();
        Self { active, offered: saved.offered, progress }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StepInfo {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) text: String,
    pub(crate) target: Target,
    pub(crate) done: Done,
    pub(crate) offer: Option<Offer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TourInfo {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) docs: String,
    pub(crate) steps: Vec<StepInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TourAnswer {
    pub(crate) state: TourState,
    pub(crate) tours: Vec<TourInfo>,
}

pub(crate) fn tours() -> Vec<TourInfo> {
    TOURS
        .iter()
        .map(|t| TourInfo {
            id: t.id.into(),
            title: t.title.into(),
            docs: t.docs_url(),
            steps: t
                .steps
                .iter()
                .map(|s| StepInfo {
                    id: s.id.into(),
                    title: s.title.into(),
                    text: s.text.into(),
                    target: s.target,
                    done: s.done,
                    offer: s.offer,
                })
                .collect(),
        })
        .collect()
}

pub(crate) fn find(id: Option<&str>) -> Result<(usize, &'static tour::Tour), String> {
    let id = id.unwrap_or(tour::CORE);
    tour::find(id).ok_or_else(|| {
        let ids: Vec<&str> = TOURS.iter().map(|t| t.id).collect();
        format!("there is no tour `{id}`; the tours are {}", ids.join(", "))
    })
}

pub(crate) fn step(tour: &tour::Tour, id: &str) -> Result<usize, String> {
    tour.steps.iter().position(|s| s.id == id).ok_or_else(|| {
        let ids: Vec<&str> = tour.steps.iter().map(|s| s.id).collect();
        format!("the tour `{}` has no step `{id}`; its steps are {}", tour.id, ids.join(", "))
    })
}

#[cfg(test)]
mod tests;
