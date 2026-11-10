use std::collections::BTreeSet;

use super::dossier::PullLive;
use super::{Place, Recap, failing, needs_you};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PresentPane {
    pub(crate) agent: String,
    pub(crate) conversation: Option<String>,
    pub(crate) pane: u64,
    pub(crate) status: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Present {
    pub(crate) panes: Vec<PresentPane>,
    pub(crate) pulls: Vec<PullLive>,
    pub(crate) boot: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Outdated {
    pub(crate) ids: BTreeSet<String>,
    pub(crate) new: bool,
}

impl Outdated {
    pub(crate) fn any(&self) -> bool {
        !self.ids.is_empty() || self.new
    }
}

fn holds(place: &Place, pane: &PresentPane, present: &Present) -> bool {
    if place.agent.as_deref() != Some(pane.agent.as_str()) {
        return false;
    }
    match &place.conversation {
        Some(conversation) => pane.conversation.as_ref() == Some(conversation),
        None => place.pane == Some(pane.pane) && place.boot.as_deref() == Some(present.boot.as_str()),
    }
}

fn holder<'a>(place: &Place, present: &'a Present) -> Option<&'a PresentPane> {
    present.panes.iter().find(|pane| holds(place, pane, present))
}

fn status_of(pane: &PresentPane) -> &str {
    pane.status.as_deref().unwrap_or("open")
}

fn conversation_changed(place: &Place, present: &Present) -> bool {
    let then = place.status.as_deref();
    let now = holder(place, present).map(status_of);
    (needs_you(then) && now != then) || (then.is_some() && now.is_none())
}

fn pull_is(pull: &PullLive, place: &Place) -> bool {
    place.workspace.as_ref().is_some_and(|named| named.path == pull.workspace) && place.number == Some(pull.number)
}

fn pull_changed(place: &Place, present: &Present) -> bool {
    failing(place.state.as_deref(), place.checks.as_deref())
        && !present.pulls.iter().any(|pull| pull_is(pull, place) && failing(Some(&pull.state), pull.checks.as_deref()))
}

fn changed(place: &Place, present: &Present) -> bool {
    match place.kind.as_str() {
        "conversation" => conversation_changed(place, present),
        "pull" => pull_changed(place, present),
        _ => false,
    }
}

fn new_question(recap: &Recap, present: &Present) -> bool {
    present.panes.iter().filter(|pane| needs_you(pane.status.as_deref())).any(|pane| {
        !recap.places.values().any(|place| {
            place.kind == "conversation"
                && holds(place, pane, present)
                && place.status.as_deref() == pane.status.as_deref()
        })
    })
}

fn named(recap: &Recap, path: &str) -> bool {
    recap.places.values().any(|place| place.workspace.as_ref().is_some_and(|named| named.path == path))
}

fn new_failure(recap: &Recap, present: &Present) -> bool {
    present
        .pulls
        .iter()
        .filter(|pull| failing(Some(&pull.state), pull.checks.as_deref()) && named(recap, &pull.workspace))
        .any(|pull| {
            !recap.places.values().any(|place| {
                place.kind == "pull" && failing(place.state.as_deref(), place.checks.as_deref()) && pull_is(pull, place)
            })
        })
}

pub(crate) fn check(recap: &Recap, present: &Present) -> Outdated {
    Outdated {
        ids: recap.places.iter().filter(|(_, place)| changed(place, present)).map(|(id, _)| id.clone()).collect(),
        new: new_question(recap, present) || new_failure(recap, present),
    }
}

#[cfg(test)]
mod tests;
