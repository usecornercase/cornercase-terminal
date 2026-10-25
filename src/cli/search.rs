use serde_json::Value;

use super::args::SearchArgs;
use super::status::tsv;
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::{self, Hit, Matches};
use crate::error::Result;

pub(super) fn run(search: &SearchArgs) -> Result<()> {
    let query = search.query.join(" ");
    let value = ask("search", control::Command::Search(control::Search { query }))?;
    if search.print.json {
        return print_json(&value);
    }
    print_out(&table(&answer::<Matches>(value)?)?)
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
