use serde_json::Value;

use super::args::SearchArgs;
use super::config::render_found;
use super::status::tsv;
use super::{print_json, print_out};
use crate::client::{answer, ask, use_search_settings};
use crate::control::config::SettingsFound;
use crate::control::{self, Hit, Matches};
use crate::error::Result;

pub(super) fn run(search: &SearchArgs) -> Result<()> {
    let query = search.query.join(" ");
    if search.settings {
        use_search_settings()?;
    }
    let value =
        ask("search", control::Command::Search(control::Search { query, settings: search.settings.then_some(true) }))?;
    if search.print.json {
        return print_json(&value);
    }
    let found = answer::<Matches>(value)?;
    if search.settings {
        return print_out(&with_settings(&found)?);
    }
    print_out(&table(&found)?)
}

fn with_settings(found: &Matches) -> Result<String> {
    let settings = render_found(&SettingsFound { results: found.settings.clone() });
    Ok(format!("{}\n{settings}", table(found)?))
}

pub(crate) fn table(found: &Matches) -> Result<String> {
    tsv(&Hit::COLUMNS, &found.matches)
}

pub(super) fn unrevealed(value: &Value, item: control::Item) -> Option<String> {
    let control::Item::Pane(pane) = item else { return None };
    (value.get("revealed") == Some(&Value::Bool(false)))
        .then(|| format!("pane {pane} is shown, but its history no longer holds that text, so nothing is selected"))
}

#[cfg(test)]
mod tests;
