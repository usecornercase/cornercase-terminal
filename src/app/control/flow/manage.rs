use std::path::{Path, PathBuf};

use super::{NO_PROJECT, origin_name};
use crate::app::App;
use crate::app::control::{Handled, conflict, json, malformed};
use crate::control::contract::{Code, Refusal};
use crate::control::{self, FlowRemoved, FlowSaved};
use crate::flows::Flow;
use crate::flows::dialog::project_risks;
use crate::flows::share::{self, To};
use crate::flows::store::{self, Listing, Origin, Saved};
use crate::paths;

impl App {
    pub(in crate::app) fn flow_save_request(&mut self, caller: Option<u64>, save: &control::FlowSave) -> Handled {
        let to = place(&save.place)?;
        let project = self.flows_scope(caller, save.project)?;
        if to == To::Project && project.is_none() {
            return Err(NO_PROJECT.into());
        }
        let flow = save.flow.clone().into_flow();
        if store::to_text(&flow).is_ok_and(|text| text.len() > share::MAX_BYTES) {
            return Err(malformed(format!("the flow is {}", share::too_large())));
        }
        let listing = self.flow_listing(project.as_deref());
        let previous = save.path.as_deref().map(|path| listed_file(&listing, path)).transpose()?;
        let dir = self.save_dir(to, project.as_deref());
        let target = save_target(&dir, &flow, previous.as_deref());
        let origin = if to == To::Yours { Origin::User(target) } else { Origin::Project(target) };
        let (problems, own) = save_problems(&flow, &listing, &origin, previous.as_deref());
        let check = (to == To::Project).then(|| project_risks(&flow)).flatten();
        let mut saved =
            FlowSaved { name: flow.name.clone(), origin: origin_name(&origin).into(), check, ..FlowSaved::default() };
        if save.check {
            saved.problems = visible_all(&problems);
            return Ok(Some(json(&saved)));
        }
        if own > 0 {
            return Err(malformed(format!("the flow has problems: {}", visible_all(&problems[..own]).join("; "))));
        }
        let path = self.save_flow_file(&dir, &flow, previous.as_deref())?;
        saved.problems = problems_after(&self.flow_listing(project.as_deref()), &path);
        saved.saved = true;
        saved.path = Some(path);
        Ok(Some(json(&saved)))
    }

    fn save_dir(&self, to: To, project: Option<&Path>) -> PathBuf {
        match (to, project) {
            (To::Project, Some(project)) => store::project_dir(project),
            _ => paths::flows(self.config_file.path()),
        }
    }

    pub(in crate::app) fn flow_delete_request(&mut self, caller: Option<u64>, delete: &control::FlowDelete) -> Handled {
        let project = self.flows_scope(caller, delete.project)?;
        let listing = self.flow_listing(project.as_deref());
        let (name, origin, path) = match &delete.path {
            Some(path) => deletable(&listing, path)?,
            None => named_file(&listing, delete)?,
        };
        self.delete_flow_file(&path)?;
        let removed = FlowRemoved { name, origin, deleted: Some(path), hidden: false, changed: true };
        Ok(Some(json(&removed)))
    }

    pub(in crate::app) fn flow_hide_request(&mut self, hide: &control::FlowHide) -> Handled {
        let listing = self.flow_listing(None);
        let name = builtin_name(&listing, &hide.name)?;
        let edit = edited(&listing, &name);
        let hidden = listing.deleted(&name).is_some();
        let mut removed = FlowRemoved { name: name.clone(), hidden: true, ..FlowRemoved::default() };
        if edit.is_none() && hidden {
            return Ok(Some(json(&removed)));
        }
        self.hide_builtin(&name, edit.as_deref())?;
        removed.deleted = edit;
        removed.changed = true;
        Ok(Some(json(&removed)))
    }

    pub(in crate::app) fn flow_restore_request(&mut self, restore: &control::FlowRestore) -> Handled {
        let listing = self.flow_listing(None);
        let name = builtin_name(&listing, &restore.name)?;
        let edit = edited(&listing, &name);
        let mut removed = FlowRemoved { name: name.clone(), ..FlowRemoved::default() };
        if edit.is_none() && listing.deleted(&name).is_none() {
            return Ok(Some(json(&removed)));
        }
        self.unhide_builtin(&name, edit.as_deref())?;
        removed.deleted = edit;
        removed.changed = true;
        Ok(Some(json(&removed)))
    }
}

fn save_target(dir: &Path, flow: &Flow, previous: Option<&Path>) -> PathBuf {
    match previous {
        Some(path) if path.parent() == Some(dir) => path.to_path_buf(),
        _ => dir.join(format!("{}.{}", store::file_name(&flow.name), store::EXTENSION)),
    }
}

fn save_problems(flow: &Flow, listing: &Listing, origin: &Origin, previous: Option<&Path>) -> (Vec<String>, usize) {
    let others: Vec<Saved> = listing.flows.iter().filter(|saved| saved.origin.path() != previous).cloned().collect();
    let mut problems = flow.problems();
    let own = problems.len();
    problems.extend(store::conflict(&others, &listing.builtins, &Saved { flow: flow.clone(), origin: origin.clone() }));
    (problems, own)
}

fn visible_all(problems: &[String]) -> Vec<String> {
    problems.iter().map(|problem| share::visible(problem)).collect()
}

fn problems_after(after: &Listing, path: &Path) -> Vec<String> {
    let found = after.flows.iter().find(|saved| saved.origin.path() == Some(path));
    found.and_then(|saved| after.conflict(saved)).iter().map(|problem| share::visible(problem)).collect()
}

fn place(text: &str) -> Result<To, Refusal> {
    match text {
        "" | "yours" => Ok(To::Yours),
        "project" => Ok(To::Project),
        other => Err(malformed(format!("`{other}` is not a place for a flow: use yours or project"))),
    }
}

fn not_found(message: String) -> Refusal {
    Refusal::new(Code::NotFound, message)
}

fn listed_file(listing: &Listing, path: &Path) -> Result<PathBuf, Refusal> {
    listing.flows.iter().find_map(|saved| saved.origin.path().filter(|p| *p == path).map(PathBuf::from)).ok_or_else(
        || {
            not_found(format!(
                "there is no flow file {}; `cornercase flow list --json` gives each flow's path",
                path.display()
            ))
        },
    )
}

fn deletable(listing: &Listing, path: &Path) -> Result<(String, Option<String>, PathBuf), Refusal> {
    if let Some(saved) = listing.flows.iter().find(|saved| saved.origin.path() == Some(path)) {
        refuse_edit(listing, saved)?;
        return Ok((saved.flow.name.clone(), Some(origin_name(&saved.origin).into()), path.to_path_buf()));
    }
    match listing.broken.iter().find(|(broken, _)| broken == path) {
        Some((broken, _)) => Ok((String::new(), None, broken.clone())),
        None => Err(not_found(format!(
            "there is no flow file {}; `cornercase flow list --json` gives each flow's path",
            path.display()
        ))),
    }
}

fn refuse_edit(listing: &Listing, saved: &Saved) -> Result<(), Refusal> {
    if store::edits_builtin(&listing.builtins, saved) {
        let name = share::visible(saved.flow.name.trim());
        return Err(conflict(format!(
            "{name} is your version of the built-in flow {name}: `cornercase flow restore {name}` deletes it and brings \
             the built-in back, `cornercase flow hide {name}` deletes it and hides the built-in"
        )));
    }
    Ok(())
}

fn named_file(listing: &Listing, delete: &control::FlowDelete) -> Result<(String, Option<String>, PathBuf), Refusal> {
    let name = delete.name.trim();
    if name.is_empty() {
        return Err(malformed("name the flow to delete, or pass its path"));
    }
    let wanted = match delete.place.as_str() {
        "" => None,
        other => Some(place(other)?),
    };
    let found: Vec<&Saved> = listing.named(name).filter(|saved| in_place(saved, wanted)).collect();
    let shown = share::visible(name);
    let Some(first) = found.first() else { return Err(no_flow_named(listing, name, &shown)) };
    if found.len() > 1 {
        return Err(several_named(&found, &shown));
    }
    refuse_edit(listing, first)?;
    let path = first.origin.path().map(PathBuf::from).unwrap_or_default();
    Ok((first.flow.name.clone(), Some(origin_name(&first.origin).into()), path))
}

fn in_place(saved: &Saved, wanted: Option<To>) -> bool {
    matches!(
        (&saved.origin, wanted),
        (Origin::User(_), None | Some(To::Yours)) | (Origin::Project(_), None | Some(To::Project))
    )
}

fn no_flow_named(listing: &Listing, name: &str, shown: &str) -> Refusal {
    if listing.deleted(name).is_some() {
        return not_found(format!(
            "the built-in flow {shown} is already deleted; `cornercase flow restore {shown}` brings it back"
        ));
    }
    if listing.named(name).any(|saved| saved.origin == Origin::BuiltIn) {
        return conflict(format!("{shown} is a built-in flow: `cornercase flow hide {shown}` deletes it"));
    }
    let names: Vec<String> = listing.flows.iter().map(|saved| share::visible(&saved.flow.name)).collect();
    not_found(format!(
        "there is no flow of yours or the project's named `{shown}`; the flows are {} (`cornercase flow list`)",
        names.join(", ")
    ))
}

fn several_named(found: &[&Saved], shown: &str) -> Refusal {
    let places: Vec<String> = found
        .iter()
        .filter_map(|saved| saved.origin.path().map(|path| format!("{}: {}", saved.origin.whose(), path.display())))
        .collect();
    conflict(format!(
        "{} flows are named `{shown}` ({}): pass --in yours or --in project, or --path FILE",
        found.len(),
        places.join("; ")
    ))
}

fn builtin_name(listing: &Listing, name: &str) -> Result<String, Refusal> {
    listing.builtins.iter().find(|built| store::same_name(built, name)).cloned().ok_or_else(|| {
        not_found(format!(
            "`{}` is not a built-in flow; the built-in flows are {}",
            share::visible(name.trim()),
            listing.builtins.join(", ")
        ))
    })
}

fn edited(listing: &Listing, name: &str) -> Option<PathBuf> {
    listing
        .flows
        .iter()
        .find(|saved| store::same_name(&saved.flow.name, name) && store::edits_builtin(&listing.builtins, saved))
        .and_then(|saved| saved.origin.path().map(PathBuf::from))
}

pub(super) fn info_listing(listing: &Listing) -> (Vec<control::FlowInfo>, Vec<control::FlowInfo>) {
    let shown = listing
        .flows
        .iter()
        .map(|saved| {
            let mut info = super::flow_info(&saved.flow, &saved.origin);
            info.problems.extend(listing.conflict(saved));
            info.edited = store::edits_builtin(&listing.builtins, saved);
            info
        })
        .collect();
    let hidden = listing.hidden.iter().map(|flow| super::flow_info(flow, &Origin::BuiltIn)).collect();
    (shown, hidden)
}
