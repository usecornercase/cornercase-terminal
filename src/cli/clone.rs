use std::borrow::Cow;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::args::CloneArgs;
use super::{clone_parent, clone_url, note, print_json, print_out};
use crate::client::{self, answer, ask};
use crate::control::{CloneFile, CloneRepository, Cloned, Command};
use crate::error::{Error, Result};
use crate::git::ignored::{self, IgnoredFile, Listing};
use crate::machines::clone_form::{self, packed};
use crate::os::ffi;

#[derive(Serialize)]
struct Offered<'a> {
    root: Cow<'a, str>,
    files: &'a [IgnoredFile],
    left_out: usize,
}

impl<'a> From<&'a Listing> for Offered<'a> {
    fn from(listing: &'a Listing) -> Self {
        Self { root: ffi::text(listing.root.path()), files: &listing.files, left_out: listing.left_out }
    }
}

pub(super) fn run(args: &CloneArgs) -> Result<()> {
    if args.list_ignored {
        return list_ignored(args);
    }
    let url = args.url.as_deref().ok_or_else(|| Error::WrongUsage("say which repository to clone".into()))?;
    let remote = client::used_remote();
    let (url, parent) = (clone_url(url, remote)?, clone_parent(args.folder.as_deref(), remote)?);
    let files = files_to_copy(args)?;
    let value = ask("clone", Command::Clone(CloneRepository { url, parent, focus: args.create.focus, files }))?;
    let cloned: Cloned = answer(value.clone())?;
    if args.create.print.json {
        print_json(&value)?;
    } else {
        print_cloned(&cloned)?;
    }
    if cloned.unwritten.is_empty() { Ok(()) } else { Err(Error::Partial) }
}

fn print_cloned(cloned: &Cloned) -> Result<()> {
    print_out(&format!("{}\n", cloned.path.display()))?;
    for file in &cloned.unwritten {
        note(&format!("not copied: {} ({})\n", file.path.display(), file.why));
    }
    Ok(())
}

fn files_to_copy(args: &CloneArgs) -> Result<Vec<CloneFile>> {
    if args.files.is_empty() {
        return Ok(Vec::new());
    }
    let from = from_folder(args)?;
    let listing = ignored::picked(&from, &args.files).map_err(|error| of_folder(&from, error))?;
    packed(&listing.root, &listing.files).map_err(|error| Error::Control(format!("cannot copy the files: {error}")))
}

fn list_ignored(args: &CloneArgs) -> Result<()> {
    let from = from_folder(args)?;
    let listing = ignored::list(&from).map_err(|error| of_folder(&from, error))?;
    if args.create.print.json {
        return print_json(&Offered::from(&listing));
    }
    print_out(&listing_text(&listing))
}

fn from_folder(args: &CloneArgs) -> Result<PathBuf> {
    match &args.from {
        Some(from) => Ok(from.clone()),
        None => Ok(std::env::current_dir()?),
    }
}

fn of_folder(from: &Path, error: Error) -> Error {
    match error {
        Error::Git(why) => Error::Control(format!("cannot read the files of `{}`: {why}", from.display())),
        error => error,
    }
}

fn listing_text(listing: &Listing) -> String {
    if listing.files.is_empty() {
        return format!("{}\n", clone_form::NONE);
    }
    let files = listing.files.iter().map(clone_form::described);
    let more = (listing.left_out > 0).then(|| clone_form::not_offered(listing.left_out));
    files.chain(more).map(|line| line + "\n").collect()
}

#[cfg(test)]
mod tests;
