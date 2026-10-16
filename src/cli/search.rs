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

#[cfg(test)]
mod tests;
