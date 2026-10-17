use super::args::view::{ViewArgs, ViewSetArgs};
use super::{print_json, print_out, stream_json};
use crate::client::{self, answer, ask};
use crate::control::{self, ViewInfo};
use crate::error::Result;

pub(super) fn open(args: &ViewArgs) -> Result<()> {
    let open = control::OpenView {
        cols: args.cols,
        rows: args.rows,
        active: args.inactive.then_some(false),
        tab: args.tab,
        workspace: args.workspace,
        project: args.project,
        visible: args.hidden.then_some(false),
    };
    if args.hidden {
        client::use_notices()?;
    }
    open.size().map_err(crate::error::Error::WrongUsage)?;
    stream_json("view", control::Command::View(open))
}

pub(super) fn set(args: &ViewSetArgs) -> Result<()> {
    let active = if args.active {
        Some(true)
    } else if args.inactive {
        Some(false)
    } else {
        None
    };
    let visible = if args.visible {
        Some(true)
    } else if args.hidden {
        Some(false)
    } else {
        None
    };
    if visible.is_some() {
        client::use_notices()?;
    }
    let set = control::SetView { cols: args.cols, rows: args.rows, active, claim: args.claim, visible };
    let value = ask("view-set", control::Command::ViewSet(set))?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&(describe(&answer::<ViewInfo>(value)?) + "\n"))
}

pub(super) fn describe(view: &ViewInfo) -> String {
    let tab = view.shown.tab.map_or_else(|| "no tab".to_string(), |tab| format!("tab {tab}"));
    let size = match (view.cols, view.rows) {
        (Some(cols), Some(rows)) => format!("{cols}×{rows}"),
        _ => "no size".to_string(),
    };
    let state = match (view.active, view.controls) {
        (false, _) => "inactive",
        (true, true) => "sizes its tab",
        (true, false) => "does not size its tab",
    };
    let hidden = if view.visible { "" } else { "  hidden" };
    format!("view {}  {tab}  {size}  {state}{hidden}", view.id)
}
